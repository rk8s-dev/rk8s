#![allow(dead_code)]
use anyhow::{Context, Result, anyhow, bail};
use dotenvy::from_path_iter;
use ipnetwork::{Ipv4Network, Ipv6Network};
use log::{error, info, warn};
use std::{path::Path, str::FromStr, sync::Arc};
use tokio::{
    signal,
    sync::{Mutex, Notify, mpsc},
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::{
    network::{
        backend::{Backend, hostgw::HostgwBackend},
        manager::LocalManager,
        registry::XlineSubnetRegistry,
    },
    protocol::config::XlineConfig,
};
use libnetwork::{
    config::NetworkConfig,
    ip::{self, PublicIPOpts},
};
use libvault::storage::xline::XlineOptions;

//const DEFAULT_SUBNET_FILE: &str = "/run/flannel/subnet.env";
const DEFAULT_SUBNET_FILE: &str = "/etc/cni/net.d/subnet.env";

pub async fn new_subnet_manager(cfg: XlineConfig, option: XlineOptions) -> Result<LocalManager> {
    let xline_registry = XlineSubnetRegistry::new_with_options(cfg, option)
        .await
        .expect("failed to create XlineSubnetRegistry");

    let registry = Arc::new(xline_registry);

    Ok(LocalManager::new(registry.clone(), None, None, 5))
}

pub async fn init_network(cfg: &mut XlineConfig, cancel_token: CancellationToken) -> Result<()> {
    const MIN_RENEW_MARGIN: i64 = 1;
    const MAX_RENEW_MARGIN: i64 = 24 * 60 - 1;

    cfg.subnet_lease_renew_margin.get_or_insert(60);

    let renew_margin = cfg.subnet_lease_renew_margin.unwrap();
    if !(MIN_RENEW_MARGIN..=MAX_RENEW_MARGIN).contains(&renew_margin) {
        bail!(
            "Invalid subnet-lease-renew-margin ({renew_margin}), must be between {MIN_RENEW_MARGIN} and {MAX_RENEW_MARGIN} minutes"
        );
    }

    let sm = match new_subnet_manager(cfg.clone(), XlineOptions::new(cfg.endpoints.clone())).await {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create subnet manager: {e:?}");
            return Err(e).context("new_subnet_manager failed");
        }
    };
    info!("Created subnet manager: {}", sm.name());

    let cancel_notify = Arc::new(Notify::new());
    let (tx, rx) = mpsc::channel::<()>(1);
    {
        let token = cancel_token.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::signal::ctrl_c().await {
                error!("failed to listen for ctrl_c: {e:?}");
                token.cancel();
                drop(tx);
                return;
            }
            info!("Received CTRL-C, cancelling...");
            token.cancel();
            drop(tx);
        });
    }

    let config = match get_config(rx, cancel_token.clone(), &sm).await {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("get_config failed: {e:?}");
            return Err(e).context("get_config failed");
        }
    };
    info!("Obtained network config");

    let ip_stack = ip::get_ip_family(config.enable_ipv4, config.enable_ipv6).map_err(|e| {
        error!("Failed to determine IP family stack: {e}");
        anyhow::anyhow!("Failed to determine IP family stack: {e}")
    })?;

    if config.enable_ipv4 && !Path::new("/proc/sys/net/bridge/bridge-nf-call-iptables").exists() {
        let err = anyhow::anyhow!(
            "br_netfilter check failed: /proc/sys/net/bridge/bridge-nf-call-iptables missing"
        );
        error!("{err}");
        return Err(err);
    }

    if config.enable_ipv6 && !Path::new("/proc/sys/net/bridge/bridge-nf-call-ip6tables").exists() {
        let err = anyhow::anyhow!(
            "br_netfilter check failed: /proc/sys/net/bridge/bridge-nf-call-ip6tables missing"
        );
        error!("{err}");
        return Err(err);
    }

    let opts_public = PublicIPOpts {
        public_ip: None,
        public_ipv6: None,
    };
    let ext_iface = ip::lookup_ext_iface(None, None, None, ip_stack, opts_public)
        .await
        .map_err(|e| {
            error!("Failed to find any valid interface to use: {e:?}");
            anyhow::anyhow!("Failed to find any valid interface to use: {e:?}")
        })?;

    info!("Selected external interface: {}", ext_iface.iface.name);

    let backend = HostgwBackend::new(ext_iface, sm.clone()).map_err(|e| {
        error!("Error creating HostGW backend: {e}");
        cancel_token.cancel();
        cancel_notify.notify_waiters();
        anyhow::anyhow!("Error creating HostGW backend: {e}")
    })?;

    let network = backend.register_network(&config).await.map_err(|e| {
        error!("Error registering network: {e}");
        cancel_token.cancel();
        cancel_notify.notify_waiters();
        anyhow::anyhow!("Error registering network: {e}")
    })?;

    let (lease, mtu) = {
        let net_guard = network.lock().await;
        let lease = net_guard.get_lease().await?;
        let mtu = net_guard.mtu().unwrap_or(1500);
        (lease, mtu)
    };
    let subnet = lease.subnet;
    let ipv6_subnet = lease.ipv6_subnet;

    if let Err(e) = sm.handle_subnet_file(
        DEFAULT_SUBNET_FILE,
        &config,
        false,
        subnet,
        ipv6_subnet,
        mtu,
    ) {
        warn!("Failed to write subnet file: {e}");
    } else {
        info!("Wrote subnet file to {DEFAULT_SUBNET_FILE}");
    }

    info!("Running backend.");
    let net_task = {
        let cancel_token = cancel_token.clone();
        let cancel_notify_net = cancel_notify.clone();
        let net = network.clone();
        tokio::spawn(async move {
            if let Err(e) = net.lock().await.run().await {
                error!("Backend network run failed: {e}");
                cancel_token.cancel();
                cancel_notify_net.notify_waiters();
            }
        })
    };

    let lease = Arc::new(Mutex::new(lease));
    if let Err(e) = sm.complete_lease(lease, cancel_notify.clone()).await {
        error!("CompleteLease execute error: {e}");
        if e.to_string().eq_ignore_ascii_case("errInterrupted") {
            cancel_token.cancel();
            cancel_notify.notify_waiters();
        }
    }

    tokio::select! {
        _ = net_task => {
            info!("Backend task exited");
        }
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            cancel_token.cancel();
            cancel_notify.notify_waiters();
        }
    }

    cancel_notify.notified().await;

    info!("Exiting cleanly...");
    Ok(())
}

pub async fn get_config(
    mut rx: mpsc::Receiver<()>,
    token: CancellationToken,
    sm: &LocalManager,
) -> Result<NetworkConfig> {
    loop {
        match sm.get_network_config().await {
            Ok(config) => {
                info!(
                    "Found network config - Backend type: {}",
                    config.backend_type
                );
                return Ok(config);
            }
            Err(err) => {
                error!("Couldn't fetch network config: {err}");
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(1)) => {},
            _ = token.cancelled() => {
                return Err(anyhow!("operation canceled"));
            }
            _ = rx.recv() => {
                info!("Received shutdown signal from channel");
                return Err(anyhow!("operation canceled"));
            }
        }
    }
}

pub fn read_cidr_from_subnet_file(path: &str, cidr_key: &str) -> Option<Ipv4Network> {
    let cidrs = read_cidrs_from_subnet_file(path, cidr_key);
    match cidrs.len() {
        0 => {
            warn!("no subnet found for key: {cidr_key} in file: {path}");
            None
        }
        1 => Some(cidrs[0]),
        _ => {
            error!(
                "error reading subnet: more than 1 entry found for key: {cidr_key} in file {path}"
            );
            None
        }
    }
}

pub fn read_cidrs_from_subnet_file(path: &str, cidr_key: &str) -> Vec<Ipv4Network> {
    let mut cidrs = Vec::new();
    if !Path::new(path).exists() {
        return cidrs;
    }

    match from_path_iter(path) {
        Ok(iter) => {
            for (key, value) in iter.flatten() {
                if key == cidr_key {
                    for s in value.split(',') {
                        match Ipv4Network::from_str(s.trim()) {
                            Ok(cidr) => cidrs.push(cidr),
                            Err(e) => error!(
                                "Couldn't parse previous {cidr_key} from subnet file at {path}: {e}"
                            ),
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Couldn't fetch previous {cidr_key} from subnet file at {path}: {e}");
        }
    }

    cidrs
}

pub fn read_ip6_cidr_from_subnet_file(path: &str, cidr_key: &str) -> Option<Ipv6Network> {
    let cidrs = read_ip6_cidrs_from_subnet_file(path, cidr_key);
    match cidrs.len() {
        0 => {
            warn!("no subnet found for key: {cidr_key} in file: {path}");
            None
        }
        1 => Some(cidrs[0]),
        _ => {
            error!(
                "error reading subnet: more than 1 entry found for key: {cidr_key} in file {path}"
            );
            None
        }
    }
}

pub fn read_ip6_cidrs_from_subnet_file(path: &str, cidr_key: &str) -> Vec<Ipv6Network> {
    let mut cidrs = Vec::new();
    if !Path::new(path).exists() {
        return cidrs;
    }

    match from_path_iter(path) {
        Ok(iter) => {
            for (key, value) in iter.flatten() {
                if key == cidr_key {
                    for s in value.split(',') {
                        match Ipv6Network::from_str(s.trim()) {
                            Ok(cidr) => cidrs.push(cidr),
                            Err(e) => error!(
                                "Couldn't parse previous {cidr_key} from subnet file at {path}: {e}"
                            ),
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Couldn't fetch previous {cidr_key} from subnet file at {path}: {e}");
        }
    }

    cidrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Once;
    use tempfile::NamedTempFile;

    fn write_subnet_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        write!(file, "{content}").expect("Failed to write to temp file");
        file
    }

    #[test]
    fn test_read_single_ipv4_cidr() {
        let file = write_subnet_file("SUBNET=10.1.2.0/24");
        let cidr = read_cidr_from_subnet_file(file.path().to_str().unwrap(), "SUBNET");
        assert_eq!(cidr, Some(Ipv4Network::from_str("10.1.2.0/24").unwrap()));
    }

    #[test]
    fn test_read_multiple_ipv4_cidrs() {
        let file = write_subnet_file("SUBNET=10.1.1.0/24,10.1.2.0/24");
        let cidr = read_cidr_from_subnet_file(file.path().to_str().unwrap(), "SUBNET");
        assert!(cidr.is_none()); // should warn & return None
        let cidrs = read_cidrs_from_subnet_file(file.path().to_str().unwrap(), "SUBNET");
        assert_eq!(cidrs.len(), 2);
        assert!(cidrs.contains(&Ipv4Network::from_str("10.1.1.0/24").unwrap()));
        assert!(cidrs.contains(&Ipv4Network::from_str("10.1.2.0/24").unwrap()));
    }

    #[test]
    fn test_read_single_ipv6_cidr() {
        let file = write_subnet_file("IP6_SUBNET=fd00::/64");
        let cidr = read_ip6_cidr_from_subnet_file(file.path().to_str().unwrap(), "IP6_SUBNET");
        assert_eq!(cidr, Some(Ipv6Network::from_str("fd00::/64").unwrap()));
    }

    #[test]
    fn test_read_multiple_ipv6_cidrs() {
        let file = write_subnet_file("IP6_SUBNET=fd00::/64,fd01::/64");
        let cidr = read_ip6_cidr_from_subnet_file(file.path().to_str().unwrap(), "IP6_SUBNET");
        assert!(cidr.is_none()); // should error & return None
        let cidrs = read_ip6_cidrs_from_subnet_file(file.path().to_str().unwrap(), "IP6_SUBNET");
        assert_eq!(cidrs.len(), 2);
        assert!(cidrs.contains(&Ipv6Network::from_str("fd00::/64").unwrap()));
        assert!(cidrs.contains(&Ipv6Network::from_str("fd01::/64").unwrap()));
    }

    #[test]
    fn test_key_not_found() {
        let file = write_subnet_file("OTHER_KEY=192.168.1.0/24");
        let cidr = read_cidr_from_subnet_file(file.path().to_str().unwrap(), "SUBNET");
        assert!(cidr.is_none());
    }

    #[test]
    fn test_invalid_cidr_string() {
        let file = write_subnet_file("SUBNET=invalid-cidr");
        let cidrs = read_cidrs_from_subnet_file(file.path().to_str().unwrap(), "SUBNET");
        assert_eq!(cidrs.len(), 0);
    }

    fn init_logging() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .format_timestamp_secs()
                .target(env_logger::Target::Stdout)
                .init();
        });
    }

    #[tokio::test]
    async fn integration_init_network_with_etcd_and_cancel() {
        init_logging();
        info!("init_network");

        let mut cfg = XlineConfig {
            endpoints: vec!["http://127.0.0.1:2379".to_string()],
            prefix: "/coreos.com/network".to_string(),
            username: None,
            password: None,
            subnet_lease_renew_margin: Some(60),
        };

        let cancel_token = CancellationToken::new();
        let ct_clone = cancel_token.clone();

        let handle = tokio::spawn(async move { init_network(&mut cfg, cancel_token).await });
        tokio::time::sleep(Duration::from_secs(5)).await;
        ct_clone.cancel();

        let res = handle.await.unwrap();
        assert!(res.is_ok(), "init_network should exit cleanly");
    }
}
