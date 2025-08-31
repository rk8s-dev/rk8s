use anyhow::Result;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use rkl::network::{
    config::{NetworkConfig, parse_network_config, validate_network_config},
    receiver::{NetworkConfigMessage, NetworkReceiver, NetworkReceiverBuilder, NetworkService},
    route::{NetworkLease, RouteConfig, RouteManager, route_equal},
    subnet::{SubnetReceiver, make_subnet_key, parse_subnet_key, write_subnet_file},
};
use std::net::IpAddr;
use tempfile::tempdir;

/// Test network configuration parsing and validation functionality
/// Verifies that JSON network configuration can be parsed correctly and validated
#[tokio::test]
async fn test_network_config_parsing() -> Result<()> {
    let config_json = r#"{
        "EnableIPv4": true,
        "EnableIPv6": false,
        "Network": "10.244.0.0/16",
        "SubnetLen": 24,
        "Backend": {"Type": "hostgw"}
    }"#;

    let mut config = parse_network_config(config_json)?;
    validate_network_config(&mut config)?;

    assert!(config.enable_ipv4);
    assert!(!config.enable_ipv6);
    assert_eq!(config.backend_type, "hostgw");
    assert_eq!(config.subnet_len, 24);

    Ok(())
}

/// Test subnet key parsing and generation operations
/// Verifies that subnet keys can be parsed from strings and generated correctly
#[tokio::test]
async fn test_subnet_key_operations() -> Result<()> {
    // Test IPv4-only subnet key parsing
    let ipv4_key = "10.244.1.0-24";
    let (ipv4_net, ipv6_net) = parse_subnet_key(ipv4_key).unwrap();
    assert_eq!(ipv4_net.to_string(), "10.244.1.0/24");
    assert!(ipv6_net.is_none());

    // Test dual-stack (IPv4 + IPv6) subnet key parsing
    let dual_key = "10.244.1.0-24&fc00::-64";
    let (ipv4_net, ipv6_net) = parse_subnet_key(dual_key).unwrap();
    assert_eq!(ipv4_net.to_string(), "10.244.1.0/24");
    assert_eq!(ipv6_net.unwrap().to_string(), "fc00::/64");

    // Test subnet key generation
    let ipv4: Ipv4Network = "10.244.1.0/24".parse()?;
    let ipv6: Ipv6Network = "fc00::/64".parse()?;

    let key_ipv4_only = make_subnet_key(&ipv4, None);
    assert_eq!(key_ipv4_only, "10.244.1.0-24");

    let key_dual = make_subnet_key(&ipv4, Some(&ipv6));
    assert_eq!(key_dual, "10.244.1.0-24&fc00::-64");

    Ok(())
}

/// Test subnet.env file writing functionality
/// Verifies that network configuration can be written to subnet.env file correctly
#[tokio::test]
async fn test_subnet_file_writing() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("subnet.env");

    let config = NetworkConfig {
        enable_ipv4: true,
        enable_ipv6: true,
        network: Some("10.244.0.0/16".parse()?),
        ipv6_network: Some("fc00::/7".parse()?),
        ..Default::default()
    };

    let ipv4_subnet: Ipv4Network = "10.244.1.0/24".parse()?;
    let ipv6_subnet: Ipv6Network = "fc00::1:0/112".parse()?;

    write_subnet_file(
        &file_path,
        &config,
        true,
        Some(ipv4_subnet),
        Some(ipv6_subnet),
        1500,
    )?;

    let contents = std::fs::read_to_string(&file_path)?;
    assert!(contents.contains("RKL_NETWORK=10.244.0.0/16"));
    assert!(contents.contains("RKL_SUBNET=10.244.1.0/24"));
    assert!(contents.contains("RKL_IPV6_NETWORK=fc00::/7"));
    assert!(contents.contains("RKL_IPV6_SUBNET=fc00::1:0/112"));
    assert!(contents.contains("RKL_MTU=1500"));
    assert!(contents.contains("RKL_IPMASQ=true"));

    Ok(())
}

/// Test route manager functionality
/// Verifies that routes can be generated from network leases correctly
#[tokio::test]
async fn test_route_manager() -> Result<()> {
    let manager = RouteManager::new(1, "hostgw".to_string());

    // Create test network lease
    let lease = NetworkLease {
        enable_ipv4: true,
        enable_ipv6: true,
        subnet: "10.244.1.0/24".parse()?,
        ipv6_subnet: Some("fc00::1:0/112".parse()?),
        public_ip: "192.168.1.100".parse()?,
        public_ipv6: Some("fc00::100".parse()?),
    };

    // Test IPv4 route generation
    let ipv4_route = manager.get_route_for_lease(&lease).unwrap();
    assert_eq!(
        ipv4_route.destination,
        IpNetwork::V4("10.244.1.0/24".parse()?)
    );
    assert_eq!(
        ipv4_route.gateway,
        Some(IpAddr::V4("192.168.1.100".parse()?))
    );

    // Test IPv6 route generation
    let ipv6_route = manager.get_v6_route_for_lease(&lease).unwrap();
    assert_eq!(
        ipv6_route.destination,
        IpNetwork::V6("fc00::1:0/112".parse()?)
    );
    assert_eq!(ipv6_route.gateway, Some(IpAddr::V6("fc00::100".parse()?)));

    Ok(())
}

/// Test route equality comparison functionality
/// Verifies that route comparison works correctly for identical and different routes
#[tokio::test]
async fn test_route_equality() -> Result<()> {
    let route1 = RouteConfig {
        destination: IpNetwork::V4("10.244.1.0/24".parse()?),
        gateway: Some(IpAddr::V4("192.168.1.1".parse()?)),
        interface_index: Some(1),
        metric: Some(100),
    };

    let route2 = RouteConfig {
        destination: IpNetwork::V4("10.244.1.0/24".parse()?),
        gateway: Some(IpAddr::V4("192.168.1.1".parse()?)),
        interface_index: Some(1),
        metric: Some(100),
    };

    let route3 = RouteConfig {
        destination: IpNetwork::V4("10.244.2.0/24".parse()?),
        gateway: Some(IpAddr::V4("192.168.1.1".parse()?)),
        interface_index: Some(1),
        metric: Some(100),
    };

    assert!(route_equal(&route1, &route2));
    assert!(!route_equal(&route1, &route3));

    Ok(())
}

/// Test network receiver builder pattern
/// Verifies that NetworkReceiver can be built with custom configuration
#[tokio::test]
async fn test_network_receiver_builder() -> Result<()> {
    let receiver = NetworkReceiverBuilder::new()
        .subnet_file_path("/tmp/test_subnet.env".to_string())
        .link_index(2)
        .backend_type("overlay".to_string())
        .node_id("test-worker-1".to_string())
        .build()?;

    assert_eq!(receiver.get_node_id(), "test-worker-1");

    Ok(())
}

/// Test subnet receiver functionality
/// Verifies that subnet configuration can be received and processed correctly
#[tokio::test]
async fn test_subnet_receiver() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("subnet.env");

    let receiver = SubnetReceiver::new(file_path.to_string_lossy().to_string());

    let config = NetworkConfig {
        enable_ipv4: true,
        enable_ipv6: false,
        network: Some("10.244.0.0/16".parse()?),
        ..Default::default()
    };

    receiver
        .handle_subnet_config(&config, true, Some("10.244.1.0/24".parse()?), None, 1500)
        .await?;

    // Verify that the subnet file was created
    assert!(file_path.exists());

    Ok(())
}

/// Test network service lifecycle management
/// Verifies that the network service can be started, handle configs, and stopped correctly
#[tokio::test]
async fn test_network_service_lifecycle() -> Result<()> {
    let receiver = NetworkReceiverBuilder::new()
        .node_id("test-worker-2".to_string())
        .build()?;

    let service = NetworkService::new(receiver);

    // Test service startup
    assert!(!service.is_running().await);
    service.start().await?;
    assert!(service.is_running().await);

    // Test configuration handling
    let config = NetworkConfigMessage::SubnetConfig {
        network_config: NetworkConfig {
            enable_ipv4: true,
            network: Some("10.244.0.0/16".parse()?),
            ..Default::default()
        },
        ip_masq: true,
        ipv4_subnet: Some("10.244.1.0/24".parse()?),
        ipv6_subnet: None,
        mtu: 1500,
    };

    service.handle_config(config).await?;

    // Test service shutdown
    service.stop().await?;
    assert!(!service.is_running().await);

    Ok(())
}

/// Test full network configuration message handling
/// Verifies that complete network configuration (subnet + routes) can be processed
#[tokio::test]
async fn test_full_network_config_message() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("subnet.env");

    let receiver = NetworkReceiverBuilder::new()
        .subnet_file_path(file_path.to_string_lossy().to_string())
        .node_id("test-worker-3".to_string())
        .build()?;

    let routes = vec![
        RouteConfig {
            destination: IpNetwork::V4("10.244.2.0/24".parse()?),
            gateway: Some(IpAddr::V4("192.168.1.100".parse()?)),
            interface_index: Some(1),
            metric: Some(100),
        },
        RouteConfig {
            destination: IpNetwork::V4("10.244.3.0/24".parse()?),
            gateway: Some(IpAddr::V4("192.168.1.101".parse()?)),
            interface_index: Some(1),
            metric: Some(100),
        },
    ];

    let full_config = NetworkConfigMessage::FullConfig {
        network_config: NetworkConfig {
            enable_ipv4: true,
            enable_ipv6: false,
            network: Some("10.244.0.0/16".parse()?),
            ..Default::default()
        },
        ip_masq: true,
        ipv4_subnet: Some("10.244.1.0/24".parse()?),
        ipv6_subnet: None,
        mtu: 1500,
        routes,
    };

    receiver.handle_network_config(full_config).await?;

    // Verify that the subnet file was created
    assert!(file_path.exists());

    Ok(())
}