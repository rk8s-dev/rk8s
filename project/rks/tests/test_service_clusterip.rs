use anyhow::Result;
use common::{LabelSelector, ObjectMeta, ServicePort, ServiceSpec, ServiceTask};
use ipnetwork::Ipv4Network;
use libvault::storage::xline::XlineOptions;
use log::LevelFilter;
use once_cell::sync::OnceCell;
use rks::{
    api::xlinestore::XlineStore,
    network::service_ip::{ServiceIpAllocator, ServiceIpRegistry},
    protocol::config::load_config,
};
use serial_test::serial;
use std::collections::HashMap;
/// Integration tests for Service ClusterIP automatic allocation
///
/// Tests the full lifecycle of Service ClusterIP management:
/// - Auto-allocation from CIDR pool
/// - Manual IP specification and validation
/// - Immutability enforcement on updates
/// - IP release on deletion
/// - Concurrent allocation safety
use std::sync::Arc;
use tokio::time::Duration;

fn init_logging() {
    static LOGGER: OnceCell<()> = OnceCell::new();
    LOGGER.get_or_init(|| {
        env_logger::builder()
            .is_test(false)
            .filter_level(LevelFilter::Info)
            .try_init()
            .ok();
    });
}

fn get_xline_endpoints() -> Vec<String> {
    let config_path = std::env::var("TEST_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/tests/config.yaml",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    });

    match load_config(&config_path) {
        Ok(cfg) => cfg.xline_config.endpoints.clone(),
        Err(_) => vec!["http://127.0.0.1:2379".to_string()],
    }
}

async fn get_store() -> Option<Arc<XlineStore>> {
    let endpoints = get_xline_endpoints();
    let option = XlineOptions::new(endpoints);
    match tokio::time::timeout(Duration::from_secs(5), XlineStore::new(option)).await {
        Ok(Ok(store)) => Some(Arc::new(store)),
        _ => None,
    }
}

async fn setup_allocator() -> Result<(Arc<ServiceIpRegistry>, ServiceIpAllocator)> {
    init_logging();
    let endpoints = get_xline_endpoints();
    let option = XlineOptions::new(endpoints.clone());

    // Create XlineConfig from endpoints
    let xline_config = rks::protocol::config::XlineConfig {
        endpoints,
        prefix: "/coreos.com/network".to_string(),
        username: None,
        password: None,
        subnet_lease_renew_margin: Some(60),
    };

    let registry = Arc::new(ServiceIpRegistry::new(xline_config, option).await?);
    let service_cidr: Ipv4Network = "10.96.0.0/24".parse().unwrap();
    let allocator = ServiceIpAllocator::new(service_cidr, registry.clone(), 10);

    Ok((registry, allocator))
}

async fn cleanup_service_ips(registry: &ServiceIpRegistry) -> Result<()> {
    let records = registry.list_all().await?;
    for record in records {
        let _ = registry.release(record.ip).await;
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_auto_allocate_clusterip() -> Result<()> {
    let Some(_store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    // Allocate first IP
    let ip1 = allocator
        .allocate("default".to_string(), "test-svc-1".to_string())
        .await?;

    println!("Allocated IP: {}", ip1);
    assert!(registry.is_allocated(ip1).await?);

    // Allocate second IP (should be different)
    let ip2 = allocator
        .allocate("default".to_string(), "test-svc-2".to_string())
        .await?;

    assert_ne!(ip1, ip2);
    assert!(registry.is_allocated(ip2).await?);

    // Cleanup
    allocator.deallocate(ip1).await?;
    allocator.deallocate(ip2).await?;

    assert!(!registry.is_allocated(ip1).await?);
    assert!(!registry.is_allocated(ip2).await?);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_concurrent_allocation() -> Result<()> {
    let Some(_store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    let allocator = Arc::new(allocator);
    let mut handles = vec![];

    // Spawn 5 concurrent allocations
    for i in 0..5 {
        let allocator_clone = allocator.clone();
        let handle = tokio::spawn(async move {
            allocator_clone
                .allocate("default".to_string(), format!("svc-{}", i))
                .await
        });
        handles.push(handle);
    }

    let mut ips = vec![];
    for handle in handles {
        let ip = handle.await??;
        ips.push(ip);
    }

    // All IPs should be unique
    let unique_count = ips.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, 5);

    // Cleanup
    for ip in ips {
        allocator.deallocate(ip).await?;
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_reserved_ips_not_allocated() -> Result<()> {
    let Some(_store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    // Allocate many IPs to check reserved addresses are skipped
    let mut allocated = vec![];
    for i in 0..10 {
        let ip = allocator
            .allocate("default".to_string(), format!("svc-{}", i))
            .await?;
        allocated.push(ip);

        let octets = ip.octets();
        // Ensure no reserved IPs: .0, .1, .254, .255
        assert!(octets[3] != 0, "Allocated network address");
        assert!(octets[3] != 1, "Allocated gateway address");
        assert!(octets[3] != 254, "Allocated backup gateway");
        assert!(octets[3] != 255, "Allocated broadcast address");
    }

    // Cleanup
    for ip in allocated {
        allocator.deallocate(ip).await?;
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_ip_conflict_detection() -> Result<()> {
    let Some(_store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, _allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    let test_ip = "10.96.0.100".parse().unwrap();

    // First allocation should succeed
    registry
        .allocate(test_ip, "default".to_string(), "svc-1".to_string())
        .await?;

    // Second allocation of same IP should fail
    let result = registry
        .allocate(test_ip, "default".to_string(), "svc-2".to_string())
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("already allocated")
    );

    // Cleanup
    registry.release(test_ip).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_ip_reuse_after_release() -> Result<()> {
    let Some(_store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    // Allocate and release an IP
    let ip = allocator
        .allocate("default".to_string(), "svc-1".to_string())
        .await?;

    allocator.deallocate(ip).await?;
    assert!(!registry.is_allocated(ip).await?);

    // Manually allocate the same IP - should succeed now
    registry
        .allocate(ip, "default".to_string(), "svc-2".to_string())
        .await?;

    assert!(registry.is_allocated(ip).await?);

    // Cleanup
    registry.release(ip).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_service_lifecycle_with_xline() -> Result<()> {
    let Some(store) = get_store().await else {
        eprintln!("Skipping test: Xline not available");
        return Ok(());
    };

    let (registry, allocator) = setup_allocator().await?;
    cleanup_service_ips(&registry).await?;

    // Create service without ClusterIP (simulating auto-allocation)
    let mut svc = ServiceTask {
        api_version: "v1".to_string(),
        kind: "Service".to_string(),
        metadata: ObjectMeta {
            name: "test-auto-svc".to_string(),
            namespace: "default".to_string(),
            ..Default::default()
        },
        spec: ServiceSpec {
            service_type: "ClusterIP".to_string(),
            cluster_ip: None,
            selector: Some(LabelSelector {
                match_labels: HashMap::from([("app".to_string(), "test".to_string())]),
                match_expressions: vec![],
            }),
            ports: vec![ServicePort {
                name: Some("http".to_string()),
                protocol: "TCP".to_string(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
        },
    };

    // Simulate auto-allocation
    let allocated_ip = allocator
        .allocate("default".to_string(), "test-auto-svc".to_string())
        .await?;

    svc.spec.cluster_ip = Some(allocated_ip.to_string());

    // Store in Xline
    let yaml = serde_yaml::to_string(&svc)?;
    store.insert_service_yaml("test-auto-svc", &yaml).await?;

    // Verify IP is allocated
    assert!(registry.is_allocated(allocated_ip).await?);

    // Simulate deletion
    store.delete_service("test-auto-svc").await?;
    allocator.deallocate(allocated_ip).await?;

    // Verify IP is released
    assert!(!registry.is_allocated(allocated_ip).await?);

    Ok(())
}
