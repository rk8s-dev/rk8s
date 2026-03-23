//! Unified Service IP management: registry, allocator, and validation.
//!
//! This module manages automatic ClusterIP allocation for Services,
//! including Xline-backed registry operations and intelligent allocation
//! with reserved address filtering and exponential backoff retry logic.

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use common::ServiceSpec;
use etcd_client::{Client, Compare, CompareOp, GetOptions, KvClient, Txn};
use ipnetwork::Ipv4Network;
use libnetwork::config::NetworkConfig;
use libvault::storage::xline::XlineOptions;
use log::{error, info, warn};
use rand::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::protocol::config::XlineConfig;

// ============================================================================
// REGISTRY: Xline-backed Service IP allocation tracking
// ============================================================================

/// Custom serializer: convert Ipv4Addr to string for JSON compatibility
fn serialize_ipv4<S>(ip: &Ipv4Addr, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&ip.to_string())
}

/// Custom deserializer: parse string back to Ipv4Addr with validation
fn deserialize_ipv4<'de, D>(deserializer: D) -> std::result::Result<Ipv4Addr, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<Ipv4Addr>().map_err(serde::de::Error::custom)
}

/// Represents the state of an allocated Service IP in the registry.
/// Stored in Xline with key format: `/registry/service-ips/{ip}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceIpRecord {
    /// The IP address that was allocated (stored as string in JSON for interop)
    #[serde(
        serialize_with = "serialize_ipv4",
        deserialize_with = "deserialize_ipv4"
    )]
    pub ip: Ipv4Addr,

    /// Service namespace that owns this IP
    pub service_namespace: String,

    /// Service name that owns this IP
    pub service_name: String,

    /// Allocation timestamp (RFC3339 format)
    pub allocated_at: String,
}

impl ServiceIpRecord {
    /// Serialize to JSON string for Xline storage
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("serializing ServiceIpRecord to JSON")
    }

    /// Deserialize from JSON string
    pub fn from_json(s: &str) -> Result<Self> {
        serde_json::from_str(s).context("deserializing ServiceIpRecord from JSON")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceIpError {
    #[error("service IP allocation failed: {0}")]
    AllocationFailed(String),

    #[error("service IP already allocated: {ip}")]
    AlreadyAllocated { ip: Ipv4Addr },

    #[error("xline error: {0}")]
    XlineError(String),

    #[error("UTF-8 decode error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Registry for managing Service IP allocations in Xline/etcd.
/// Each allocated Service IP is stored as a key-value pair with CAS versioning for atomicity.
pub struct ServiceIpRegistry {
    kv: Arc<Mutex<KvClient>>,
}

impl fmt::Debug for ServiceIpRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceIpRegistry").finish()
    }
}

impl ServiceIpRegistry {
    /// Create a new ServiceIpRegistry connecting to Xline endpoints.
    pub async fn new(cfg: XlineConfig, options: XlineOptions) -> Result<Self> {
        let endpoints = if options.endpoints.is_empty() {
            cfg.endpoints.clone()
        } else {
            options.endpoints.clone()
        };

        let mut connect_opts = options.config.clone().unwrap_or_default();
        if let (Some(user), Some(pass)) = (&cfg.username, &cfg.password) {
            connect_opts = connect_opts.with_user(user.clone(), pass.clone());
        }

        let client = Client::connect(endpoints, Some(connect_opts))
            .await
            .context("failed to connect to Xline for ServiceIpRegistry")?;

        let kv = client.kv_client();

        Ok(Self {
            kv: Arc::new(Mutex::new(kv)),
        })
    }

    /// Generate the Xline key for a Service IP record
    fn make_key(ip: Ipv4Addr) -> String {
        format!("/registry/service-ips/{}", ip)
    }

    /// Allocate (reserve) a Service IP in the registry using CAS (compare-and-set).
    /// Returns success only if the IP was not previously allocated.
    pub async fn allocate(
        &self,
        ip: Ipv4Addr,
        service_namespace: String,
        service_name: String,
    ) -> std::result::Result<(), ServiceIpError> {
        let key = Self::make_key(ip);
        let record = ServiceIpRecord {
            ip,
            service_namespace,
            service_name,
            allocated_at: Utc::now().to_rfc3339(),
        };
        let value = record
            .to_json()
            .map_err(|e| ServiceIpError::AllocationFailed(e.to_string()))?;

        // Use CAS to ensure atomic allocation: only succeed if key version is 0 (doesn't exist)
        let cmp = Compare::version(key.clone(), CompareOp::Equal, 0);
        let put_op = etcd_client::TxnOp::put(key.clone(), value, None);
        let txn = Txn::new().when([cmp]).and_then([put_op]);

        let mut kv = self.kv.lock().await;
        let txn_resp = kv
            .txn(txn)
            .await
            .map_err(|e| ServiceIpError::XlineError(e.to_string()))?;

        if txn_resp.succeeded() {
            info!("allocated service IP {}", ip);
            Ok(())
        } else {
            warn!("CAS failed for IP {} (already allocated)", ip);
            Err(ServiceIpError::AlreadyAllocated { ip })
        }
    }

    /// Release (delete) a Service IP allocation from the registry.
    pub async fn release(&self, ip: Ipv4Addr) -> std::result::Result<(), ServiceIpError> {
        let key = Self::make_key(ip);
        let mut kv = self.kv.lock().await;

        kv.delete(key.clone(), None)
            .await
            .map_err(|e| ServiceIpError::XlineError(e.to_string()))?;

        info!("released service IP {}", ip);
        Ok(())
    }

    /// Check if a Service IP is allocated in the registry.
    #[allow(dead_code)]
    pub async fn is_allocated(&self, ip: Ipv4Addr) -> std::result::Result<bool, ServiceIpError> {
        let key = Self::make_key(ip);
        let mut kv = self.kv.lock().await;

        let resp = kv
            .get(key, None)
            .await
            .map_err(|e| ServiceIpError::XlineError(e.to_string()))?;

        Ok(!resp.kvs().is_empty())
    }

    /// Get allocation record for a Service IP.
    pub async fn get_record(
        &self,
        ip: Ipv4Addr,
    ) -> std::result::Result<Option<ServiceIpRecord>, ServiceIpError> {
        let key = Self::make_key(ip);
        let mut kv = self.kv.lock().await;

        let resp = kv
            .get(key, None)
            .await
            .map_err(|e| ServiceIpError::XlineError(e.to_string()))?;

        let Some(kv_pair) = resp.kvs().first() else {
            return Ok(None);
        };

        let value_str = std::str::from_utf8(kv_pair.value())?;
        let record = ServiceIpRecord::from_json(value_str)
            .map_err(|e| ServiceIpError::AllocationFailed(e.to_string()))?;
        Ok(Some(record))
    }

    /// List all allocated Service IPs in the registry (prefix scan).
    pub async fn list_all(&self) -> std::result::Result<Vec<ServiceIpRecord>, ServiceIpError> {
        let prefix = "/registry/service-ips/";
        let mut kv = self.kv.lock().await;

        let resp = kv
            .get(prefix, Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| ServiceIpError::XlineError(e.to_string()))?;

        let mut records = Vec::new();
        for kv_pair in resp.kvs() {
            let value_str = std::str::from_utf8(kv_pair.value())
                .context("service IP record value is not valid UTF-8")?;
            match ServiceIpRecord::from_json(value_str) {
                Ok(record) => records.push(record),
                Err(e) => {
                    error!("failed to deserialize service IP record: {}", e);
                }
            }
        }

        Ok(records)
    }
}

// ============================================================================
// ALLOCATOR: Intelligent IP allocation with retry and reserved address filtering
// ============================================================================

/// Reserved IP addresses in an IPv4 subnet that cannot be allocated.
/// Based on Kubernetes and RFC standards:
/// - .0: network address
/// - first host of the whole ServiceCIDR: gateway/API server
/// - .254: typically reserved for gateways
/// - .255: broadcast address
pub fn is_reserved_service_ip(ip: Ipv4Addr, subnet: Ipv4Network) -> bool {
    // Network address (.0)
    if ip == subnet.ip() {
        return true;
    }

    // Broadcast address (.255)
    if ip == subnet.broadcast() {
        return true;
    }

    // First host address of the whole CIDR (often used as gateway/API server)
    let first_host = Ipv4Addr::from(u32::from(subnet.ip()) + 1);
    if ip == first_host {
        return true;
    }

    let octets = ip.octets();

    // .254 for /24 subnets (often used as secondary gateway)
    if subnet.prefix() == 24 && octets[3] == 254 {
        return true;
    }

    false
}

/// Allocates Service ClusterIPs with exponential backoff retry on conflicts.
#[derive(Debug, Clone)]
pub struct ServiceIpAllocator {
    service_cidr: Ipv4Network,
    pub registry: Arc<ServiceIpRegistry>,
    /// Exponential backoff state: (initial_ms, max_ms)
    backoff_config: (u64, u64),
    /// Maximum retry attempts before giving up
    max_retries: usize,
}

impl ServiceIpAllocator {
    /// Create a new ServiceIpAllocator.
    pub fn new(
        service_cidr: Ipv4Network,
        registry: Arc<ServiceIpRegistry>,
        max_retries: usize,
    ) -> Self {
        Self {
            service_cidr,
            registry,
            backoff_config: (10, 200), // 10ms initial, 200ms max
            max_retries,
        }
    }

    /// Allocate a Service IP from the CIDR range.
    /// Retries with exponential backoff if CAS conflicts occur.
    pub async fn allocate(
        &self,
        service_namespace: String,
        service_name: String,
    ) -> Result<Ipv4Addr> {
        for attempt in 0..self.max_retries {
            // Generate candidate IPs (creates and drops RNG before any await)
            let candidates = {
                let mut rng = rand::rng();
                self.generate_candidate_ips(&mut rng)?
            };

            if candidates.is_empty() {
                return Err(anyhow!(
                    "Service IP pool exhausted (no available IPs in {})",
                    self.service_cidr
                ));
            }

            // Try multiple candidates in this attempt before backing off.
            let mut had_conflict = false;
            for candidate_ip in candidates {
                match self
                    .registry
                    .allocate(
                        candidate_ip,
                        service_namespace.clone(),
                        service_name.clone(),
                    )
                    .await
                {
                    Ok(()) => {
                        info!(
                            "Service IP allocation succeeded (attempt {}): {}",
                            attempt, candidate_ip
                        );
                        return Ok(candidate_ip);
                    }
                    Err(ServiceIpError::AlreadyAllocated { .. }) => {
                        had_conflict = true;
                        continue;
                    }
                    Err(e) => {
                        error!("allocation error (attempt {}): {}", attempt, e);
                        return Err(anyhow!("Service IP allocation failed: {}", e));
                    }
                }
            }

            if had_conflict {
                warn!(
                    "service IP allocation attempt {} had conflicts, retrying with backoff",
                    attempt
                );
                self.apply_backoff(attempt).await;
            }
        }

        if let Some(ip) = self
            .allocate_with_linear_probe(service_namespace, service_name)
            .await?
        {
            return Ok(ip);
        }

        Err(anyhow!(
            "failed to allocate Service IP after {} attempts",
            self.max_retries
        ))
    }

    /// Deallocate (release) a Service IP back to the pool.
    pub async fn deallocate(&self, ip: Ipv4Addr) -> Result<()> {
        self.registry
            .release(ip)
            .await
            .context("failed to release Service IP in registry")?;
        info!("service IP deallocated: {}", ip);
        Ok(())
    }

    /// Generate candidate Service IPs for allocation.
    ///
    /// This samples from the full CIDR range to avoid hot-spotting on the first few IPs.
    fn generate_candidate_ips(&self, rng: &mut impl Rng) -> Result<Vec<Ipv4Addr>> {
        const MAX_CANDIDATES: usize = 256;
        const MAX_SAMPLE_ATTEMPTS: usize = 4096;

        let network = u32::from(self.service_cidr.network());
        let broadcast = u32::from(self.service_cidr.broadcast());

        if broadcast <= network + 1 {
            return Err(anyhow!("no usable host IPs in CIDR {}", self.service_cidr));
        }

        let mut candidates = Vec::with_capacity(MAX_CANDIDATES);
        let mut seen = HashSet::with_capacity(MAX_CANDIDATES * 2);

        for _ in 0..MAX_SAMPLE_ATTEMPTS {
            if candidates.len() >= MAX_CANDIDATES {
                break;
            }

            let v = rng.random_range((network + 1)..broadcast);
            if !seen.insert(v) {
                continue;
            }

            let ip = Ipv4Addr::from(v);
            if is_reserved_service_ip(ip, self.service_cidr) {
                continue;
            }

            candidates.push(ip);
        }

        if candidates.is_empty() {
            return Err(anyhow!("no available IPs in CIDR {}", self.service_cidr));
        }

        Ok(candidates)
    }

    /// Fallback linear probe to avoid false "pool exhausted" in larger subnets.
    ///
    /// For subnets up to /16, this probes all usable hosts in randomized order.
    /// For larger subnets, probing is capped for latency protection.
    async fn allocate_with_linear_probe(
        &self,
        service_namespace: String,
        service_name: String,
    ) -> Result<Option<Ipv4Addr>> {
        let network = u32::from(self.service_cidr.network());
        let broadcast = u32::from(self.service_cidr.broadcast());
        if broadcast <= network + 1 {
            return Ok(None);
        }

        let usable_hosts = (broadcast - network - 1) as usize;
        let probe_limit = if usable_hosts <= 65_536 {
            usable_hosts
        } else {
            65_536
        };

        if probe_limit == 0 {
            return Ok(None);
        }

        let start = {
            let mut rng = rand::rng();
            rng.random_range(0..probe_limit)
        };

        for offset in 0..probe_limit {
            let host_idx = ((start + offset) % probe_limit) as u32;
            let ip = Ipv4Addr::from(network + 1 + host_idx);
            if is_reserved_service_ip(ip, self.service_cidr) {
                continue;
            }

            match self
                .registry
                .allocate(ip, service_namespace.clone(), service_name.clone())
                .await
            {
                Ok(()) => {
                    info!("Service IP allocation succeeded with linear probe: {}", ip);
                    return Ok(Some(ip));
                }
                Err(ServiceIpError::AlreadyAllocated { .. }) => continue,
                Err(e) => return Err(anyhow!("Service IP allocation failed: {}", e)),
            }
        }

        Ok(None)
    }

    /// Apply exponential backoff before retry.
    async fn apply_backoff(&self, attempt: usize) {
        let (initial, max) = self.backoff_config;
        let mut backoff = initial * 2_u64.pow(attempt.min(6) as u32);
        backoff = backoff.min(max);

        // Add jitter: 0.8x - 1.2x (compute before await)
        let jittered = {
            let mut rng = rand::rng();
            let jitter_factor: f64 = 0.8 + (rng.random::<f64>() * 0.4);
            (backoff as f64 * jitter_factor) as u64
        };

        sleep(StdDuration::from_millis(jittered)).await;
    }
}

// ============================================================================
// VALIDATION: Service spec validation and lifecycle management
// ============================================================================

/// Validates and processes a Service spec for ClusterIP assignment.
pub async fn validate_and_allocate_cluster_ip(
    service_namespace: &str,
    service_name: &str,
    spec: &mut ServiceSpec,
    network_config: &NetworkConfig,
    registry: &ServiceIpRegistry,
    allocator: &ServiceIpAllocator,
) -> Result<()> {
    // Only process ClusterIP service type
    if spec.service_type != "ClusterIP" {
        return Ok(());
    }

    // Headless service is identified by `clusterIP: None` (Kubernetes semantics).
    if let Some(cluster_ip) = spec.cluster_ip.as_ref()
        && cluster_ip.eq_ignore_ascii_case("none")
    {
        info!(
            "Service {}/{}: headless service (clusterIP=None), skip ClusterIP allocation",
            service_namespace, service_name
        );
        return Ok(());
    }

    // Get Service CIDR from network config
    let service_cidr = network_config
        .service_cidr
        .ok_or_else(|| anyhow!("ServiceCIDR not configured in network config"))?;

    match spec.cluster_ip.as_ref().map(|s| s.trim()) {
        // Case 1: cluster_ip is not set - auto-allocate
        None | Some("") => {
            let allocated_ip = allocator
                .allocate(service_namespace.to_string(), service_name.to_string())
                .await
                .context("failed to allocate Service IP")?;

            info!(
                "Service {}/{}: allocated ClusterIP {}",
                service_namespace, service_name, allocated_ip
            );
            spec.cluster_ip = Some(allocated_ip.to_string());
            Ok(())
        }
        // Case 2: cluster_ip is set - validate format and range
        Some(ip_str) => {
            // Parse and validate IP format
            let ip: Ipv4Addr = ip_str
                .parse()
                .context(format!("invalid cluster_ip format: {}", ip_str))?;

            // Validate IP is in Service CIDR range
            if !service_cidr.contains(ip) {
                return Err(anyhow!(
                    "cluster_ip {} is outside Service CIDR range ({})",
                    ip,
                    service_cidr
                ));
            }

            if is_reserved_service_ip(ip, service_cidr) {
                return Err(anyhow!(
                    "cluster_ip {} is a reserved address in Service CIDR ({})",
                    ip,
                    service_cidr
                ));
            }

            // If already allocated, allow only when owned by the same service.
            if let Some(record) = registry.get_record(ip).await? {
                if record.service_namespace == service_namespace
                    && record.service_name == service_name
                {
                    info!(
                        "Service {}/{}: cluster_ip {} already reserved by self",
                        service_namespace, service_name, ip
                    );
                    return Ok(());
                }

                return Err(anyhow!(
                    "cluster_ip {} is already allocated to another service ({}/{})",
                    ip,
                    record.service_namespace,
                    record.service_name
                ));
            }

            info!(
                "Service {}/{}: cluster_ip {} validated and reserved",
                service_namespace, service_name, ip
            );

            // Reserve the IP in the registry
            registry
                .allocate(ip, service_namespace.to_string(), service_name.to_string())
                .await
                .context(format!("failed to reserve cluster_ip {}", ip))?;

            Ok(())
        }
    }
}

/// Validates that cluster_ip cannot be modified (Kubernetes immutability rule).
pub fn validate_cluster_ip_immutability(
    service_name: &str,
    old_spec: &ServiceSpec,
    new_spec: &ServiceSpec,
) -> Result<()> {
    let normalize_cluster_ip = |v: &Option<String>| {
        v.as_ref().map(|s| s.trim()).and_then(|s| {
            if s.is_empty() {
                None
            } else if s.eq_ignore_ascii_case("none") {
                Some("None".to_string())
            } else {
                Some(s.to_string())
            }
        })
    };

    let old_cluster_ip = normalize_cluster_ip(&old_spec.cluster_ip);
    let new_cluster_ip = normalize_cluster_ip(&new_spec.cluster_ip);

    // ClusterIP services: cluster_ip is immutable
    if old_spec.service_type == "ClusterIP"
        && new_spec.service_type == "ClusterIP"
        && old_cluster_ip.is_some()
        && old_cluster_ip != new_cluster_ip
    {
        return Err(anyhow!(
            "cluster_ip for service {} cannot be modified: old={:?}, new={:?}",
            service_name,
            old_spec.cluster_ip,
            new_spec.cluster_ip
        ));
    }

    Ok(())
}

/// Deallocates the Service IP when a service is deleted.
pub async fn deallocate_cluster_ip(
    service_name: &str,
    spec: &ServiceSpec,
    allocator: &ServiceIpAllocator,
) -> Result<()> {
    // Only deallocate when cluster_ip is a real allocatable IP.
    let cluster_ip_str = match spec.cluster_ip.as_ref().map(|s| s.trim()) {
        Some(ip) if !ip.is_empty() && !ip.eq_ignore_ascii_case("none") => ip,
        _ => return Ok(()),
    };

    // Parse the IP address
    let ip: Ipv4Addr = cluster_ip_str.parse().context(format!(
        "invalid cluster_ip format during deletion: {}",
        cluster_ip_str
    ))?;

    // Release from allocator/registry
    allocator.deallocate(ip).await?;
    info!("Service {}: released ClusterIP {}", service_name, ip);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserved_service_ips() {
        let subnet: Ipv4Network = "10.96.0.0/24".parse().unwrap();

        assert!(is_reserved_service_ip(Ipv4Addr::new(10, 96, 0, 0), subnet));
        assert!(is_reserved_service_ip(Ipv4Addr::new(10, 96, 0, 1), subnet));
        assert!(is_reserved_service_ip(
            Ipv4Addr::new(10, 96, 0, 254),
            subnet
        ));
        assert!(is_reserved_service_ip(
            Ipv4Addr::new(10, 96, 0, 255),
            subnet
        ));

        assert!(!is_reserved_service_ip(Ipv4Addr::new(10, 96, 0, 2), subnet));
        assert!(!is_reserved_service_ip(
            Ipv4Addr::new(10, 96, 0, 100),
            subnet
        ));
        assert!(!is_reserved_service_ip(
            Ipv4Addr::new(10, 96, 0, 200),
            subnet
        ));
    }

    #[test]
    fn test_first_host_only_reserved_for_broad_cidr() {
        let subnet: Ipv4Network = "10.96.0.0/16".parse().unwrap();

        // First host of the whole /16 should be reserved.
        assert!(is_reserved_service_ip(Ipv4Addr::new(10, 96, 0, 1), subnet));

        // Other *.1 addresses in the same /16 should remain allocatable.
        assert!(!is_reserved_service_ip(Ipv4Addr::new(10, 96, 1, 1), subnet));
        assert!(!is_reserved_service_ip(
            Ipv4Addr::new(10, 96, 200, 1),
            subnet
        ));
    }

    #[test]
    fn test_immutability_validation() {
        use common::LabelSelector;

        let old_spec = ServiceSpec {
            cluster_ip: Some("10.96.0.100".to_string()),
            service_type: "ClusterIP".to_string(),
            selector: Some(LabelSelector {
                match_labels: std::collections::HashMap::new(),
                match_expressions: vec![],
            }),
            ports: vec![],
        };

        let mut new_spec = old_spec.clone();
        // Same IP should pass
        assert!(validate_cluster_ip_immutability("test-svc", &old_spec, &new_spec).is_ok());

        // Different IP should fail
        new_spec.cluster_ip = Some("10.96.0.101".to_string());
        assert!(validate_cluster_ip_immutability("test-svc", &old_spec, &new_spec).is_err());
    }
}
