#[cfg(test)]
mod tests {
    use crate::rust_cni::{cni::Libcni, namespace::Namespace};
    use log::{LevelFilter, debug, error, info, warn};
    use once_cell::sync::OnceCell;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    fn init_logger() {
        static LOGGER: OnceCell<()> = OnceCell::new();
        LOGGER.get_or_init(|| {
            env_logger::builder()
                .filter_level(LevelFilter::Trace)
                .is_test(true)
                .init();
        });
    }

    // test const
    const TEST_NETWORK_CONF: &str = r#"{
        "cniVersion": "0.4.0",
        "name": "test-network",
        "type": "bridge",
        "bridge": "cni-test-br0",
        "isGateway": true,
        "ipMasq": true,
        "ipam": {
            "type": "host-local",
            "subnet": "192.168.2.0/24",
            "gateway": "192.168.2.1"
        }
    }"#;

    const TEST_NETWORK_CONF_2: &str = r#"{
    "cniVersion": "0.4.0",
    "name": "test-network2",
    "plugins": [
        {
        "type": "bridge",
        "bridge": "cni0",
        "isGateway": true,
        "ipMasq": true,
        "promiscMode": true,
        "ipam": {
            "type": "host-local",
            "ranges": [
            [{
                "subnet": "192.168.1.0/24"
            }],
            [{
                "subnet": "2001:4860:4860::/64"
            }]
            ],
            "routes": [
            { "dst": "0.0.0.0/0" },
            { "dst": "::/0" }
            ]
        }
        },
        {
        "type": "firewall",
        "ingressPolicy": "same-bridge"
        },
        {
        "type": "portmap",
        "capabilities": {"portMappings": true}
        }
    ]
    }"#;

    // test helper function
    fn setup_test_environment(cnf: &str) -> std::io::Result<String> {
        let test_dir = format!("/tmp/cni-test-{}", uuid::Uuid::new_v4());
        fs::create_dir_all(&test_dir)?;
        if cnf.contains("plugins") {
            let config_path = format!("{}/10-test-network.conflist", test_dir);
            fs::write(&config_path, cnf)?;
        } else {
            let config_path = format!("{}/10-test-network.conf", test_dir);
            fs::write(&config_path, cnf)?;
        }

        info!("Created test environment at {}", test_dir);
        Ok(test_dir)
    }

    // test helper function
    fn cleanup_test_environment(dir: &str) -> std::io::Result<()> {
        fs::remove_dir_all(dir)?;
        info!("Cleaned up test environment at {dir}");
        Ok(())
    }

    // test helper function
    fn create_netns(name: &str) -> Result<String, String> {
        // ensure netns dir exists
        let netns_dir = "/var/run/netns";
        if !Path::new(netns_dir).exists() {
            fs::create_dir_all(netns_dir)
                .map_err(|e| format!("Failed to create netns dir: {e}"))?;
        }

        // create netns
        let output = Command::new("ip")
            .args(&["netns", "add", name])
            .output()
            .map_err(|e| format!("Failed to create netns: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to create netns: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let path = format!("/var/run/netns/{name}");
        info!("Created network namespace {} at {name}", path);
        Ok(path)
    }

    // test helper function
    fn delete_netns(name: &str) -> Result<(), String> {
        let output = Command::new("ip")
            .args(&["netns", "delete", name])
            .output()
            .map_err(|e| format!("Failed to delete netns: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to delete netns: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        info!("Deleted network namespace {name}");
        Ok(())
    }

    // test: initialize CNI and load config
    #[test]
    fn test_cni_initialization() {
        init_logger();

        info!("Starting CNI initialization test");

        // create test environment
        let test_dir = match setup_test_environment(TEST_NETWORK_CONF) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to setup test environment: {e}");
                panic!("Test setup failed");
            }
        };

        // create and initialize cni
        let mut cni = Libcni::new(
            Some(vec!["/opt/cni/bin".to_string()]),
            Some(test_dir.clone()),
            Some("/tmp/cni-cache".to_string()),
        );

        cni.load_default_conf();

        // validate networks loaded
        let networks = cni.get_networks();
        assert!(
            !networks.is_empty(),
            "Should have loaded at least one network"
        );

        let network = &networks[0];
        assert_eq!(
            network.config.name, "test-network",
            "Network name should match"
        );

        info!("CNI initialization test completed successfully");

        // cleanup test environment
        if let Err(e) = cleanup_test_environment(&test_dir) {
            warn!("Failed to cleanup test environment: {e}");
        }
    }

    // test: complete network lifecycle
    #[test]
    fn test_network_lifecycle() {
        init_logger();

        info!("Starting network lifecycle test");

        let test_dir = match setup_test_environment(TEST_NETWORK_CONF) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to setup test environment: {e}");
                panic!("Test setup failed");
            }
        };

        let ns_name = format!("cni-test-{}", uuid::Uuid::new_v4());
        let ns_path = match create_netns(&ns_name) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to create network namespace: {e}");
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to create netns");
            }
        };

        let mut cni = Libcni::new(
            Some(vec!["/opt/cni/bin".to_string()]),
            Some(test_dir.clone()),
            Some("/tmp/cni-cache".to_string()),
        );

        cni.load_default_conf();

        // create container id and network namespace
        let container_id = format!("container-{}", uuid::Uuid::new_v4());

        // setup network
        match cni.setup(container_id.clone(), ns_path.clone()) {
            Ok(_) => info!("Network setup successful"),
            Err(e) => {
                error!("Network setup failed: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Network setup failed");
            }
        }

        // check network  it should be notion now, because this interface is not supported now
        // match cni.check(container_id.clone(), ns_path.clone()) {
        //     Ok(_) => info!("Network check successful"),
        //     Err(e) => {
        //         error!("Network check failed: {e}");
        //         cni.remove(container_id.clone(), ns_path.clone())
        //             .unwrap_or_default();
        //         delete_netns(&ns_name).unwrap_or_default();
        //         cleanup_test_environment(&test_dir).unwrap_or_default();
        //         panic!("Network check failed");
        //     }
        // }

        // create namespace with args
        let mut custom_ns = Namespace::new(container_id.clone(), ns_path.clone());
        custom_ns.add_arg("IgnoreUnknown", "true");
        custom_ns.add_capability("portMappings", r#"[{"hostPort":8080,"containerPort":80}]"#);

        // remove network
        match cni.remove(container_id.clone(), ns_path.clone()) {
            Ok(_) => info!("Network removal successful"),
            Err(e) => {
                error!("Network removal failed: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Network removal failed");
            }
        }

        // cleanup
        if let Err(e) = delete_netns(&ns_name) {
            warn!("Failed to delete network namespace: {e}");
        }

        if let Err(e) = cleanup_test_environment(&test_dir) {
            warn!("Failed to cleanup test environment: {e}");
        }

        info!("Network lifecycle test completed successfully");
    }

    // test: error handling
    #[test]
    fn test_error_handling() {
        init_logger();

        info!("Starting error handling test");

        // create cni with invalid config
        let cni = Libcni::new(
            Some(vec!["/non-existent-path".to_string()]),
            Some("/non-existent-dir".to_string()),
            Some("/tmp/cni-cache".to_string()),
        );

        // test invalid container id
        let container_id = "";
        let ns_path = "/non-existent-ns";

        let result = cni.setup(container_id.to_string(), ns_path.to_string());
        assert!(result.is_err(), "Should fail with invalid container ID");

        info!("Error handling test completed successfully");
    }

    // test: use custom network config
    #[test]
    fn test_custom_network_config() {
        init_logger();

        info!("Starting custom network config test");

        // create test directory
        let test_dir = match setup_test_environment(TEST_NETWORK_CONF) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to setup test environment: {e}");
                panic!("Test setup failed");
            }
        };

        // create network namespace
        let ns_name = format!("cni-test-{}", uuid::Uuid::new_v4());
        let ns_path = match create_netns(&ns_name) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to create network namespace: {e}");
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to create netns");
            }
        };

        // create CNI instance
        let mut cni = Libcni::new(
            Some(vec!["/opt/cni/bin".to_string()]),
            Some(test_dir.clone()),
            Some("/tmp/cni-cache".to_string()),
        );

        // load custom network config
        cni.load_default_conf();

        // add loopback network
        match cni.add_lo_network() {
            Ok(_) => info!("Added loopback network"),
            Err(e) => {
                error!("Failed to add loopback network: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to add loopback network");
            }
        }

        // create container id
        let container_id = format!("lo-container-{}", uuid::Uuid::new_v4());

        // setup network
        match cni.setup(container_id.clone(), ns_path.clone()) {
            Ok(_) => info!("Network setup with loopback successful"),
            Err(e) => {
                error!("Network setup failed: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Network setup failed");
            }
        }

        // cleanup
        match cni.remove(container_id, ns_path) {
            Ok(_) => debug!("Network cleanup successful"),
            Err(e) => warn!("Network cleanup failed: {e}"),
        }

        if let Err(e) = delete_netns(&ns_name) {
            warn!("Failed to delete network namespace: {e}");
        }

        if let Err(e) = cleanup_test_environment(&test_dir) {
            warn!("Failed to cleanup test environment: {e}");
        }

        debug!("Custom network config test completed successfully");
    }

    // test: cached is removed
    #[test]
    fn test_cni_files_is_removed() {
        init_logger();

        info!("Starting cached is removed test");

        let test_dir = match setup_test_environment(TEST_NETWORK_CONF) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to setup test environment: {e}");
                panic!("Test setup failed");
            }
        };

        let ns_name = format!("cni-test-{}", uuid::Uuid::new_v4());
        let ns_path = match create_netns(&ns_name) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to create network namespace: {e}");
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to create netns");
            }
        };

        let mut cni = Libcni::new(
            Some(vec!["/opt/cni/bin".to_string()]),
            Some(test_dir.clone()),
            Some("/tmp/cni-cache".to_string()),
        );

        cni.load_default_conf();

        let container_id = format!("cached-container-{}", uuid::Uuid::new_v4());

        // setup network
        match cni.setup(container_id.clone(), ns_path.clone()) {
            Ok(_) => info!("Network setup successful"),
            Err(e) => {
                error!("Network setup failed: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Network setup failed");
            }
        }

        // cleanup
        match cni.remove(container_id, ns_path) {
            Ok(_) => debug!("Network cleanup successful"),
            Err(e) => warn!("Network cleanup failed: {e}"),
        }

        // check if cache is removed
        let cache_dir = Path::new("/tmp/cni-cache");

        // check if cni config files are removed
        let cni_config_files = cache_dir
            .read_dir()
            .unwrap()
            .filter_map(|entry| {
                if let Ok(entry) = entry {
                    if entry.path().is_file() {
                        Some(entry.path())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        assert!(
            cni_config_files.is_empty(),
            "CNI config files should be removed"
        );
    }

    // test: custom network config with multiple plugins
    #[test]
    fn test_custom_network_config_with_plugins() {
        init_logger();

        info!("Starting custom network config test with multiple plugins");

        let test_dir = match setup_test_environment(TEST_NETWORK_CONF_2) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to setup test environment: {e}");
                panic!("Test setup failed");
            }
        };

        // create network namespace
        let ns_name = format!("cni-test-{}", uuid::Uuid::new_v4());
        let ns_path = match create_netns(&ns_name) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to create network namespace: {e}");
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to create netns");
            }
        };

        // create cni instance
        let mut cni = Libcni::new(
            Some(vec!["/opt/cni/bin".to_string()]),
            Some(test_dir.clone()),
            Some("/tmp/cni-cache".to_string()),
        );

        // load custom network config
        cni.load_default_conf();
        match cni.add_lo_network() {
            Ok(_) => info!("Added loopback network"),
            Err(e) => {
                error!("Failed to add loopback network: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Failed to add loopback network");
            }
        }
        // create container id
        let container_id = format!("multi-plugin-container-{}", uuid::Uuid::new_v4());

        // setup network
        match cni.setup(container_id.clone(), ns_path.clone()) {
            Ok(_) => info!("Network setup with multiple plugins successful"),
            Err(e) => {
                error!("Network setup failed: {e}");
                delete_netns(&ns_name).unwrap_or_default();
                cleanup_test_environment(&test_dir).unwrap_or_default();
                panic!("Network setup failed: {e}");
            }
        }

        // cleanup network
        match cni.remove(container_id, ns_path) {
            Ok(_) => debug!("Network cleanup successful"),
            Err(e) => warn!("Network cleanup failed: {e}"),
        }

        // cleanup resources
        if let Err(e) = delete_netns(&ns_name) {
            warn!("Failed to delete network namespace: {e}");
        }

        if let Err(e) = cleanup_test_environment(&test_dir) {
            warn!("Failed to cleanup test environment: {e}");
        }

        info!("Custom network config test with multiple plugins completed successfully");
    }
}
