use libvault::storage::xline::XlineOptions;
use rks::api::xlinestore::XlineStore;
use rks::dns::authority::run_dns_server;
use rks::protocol::config::load_config;
use std::{fs, sync::Arc};

use env_logger;
use log::{LevelFilter, info};
use once_cell::sync::OnceCell;

fn init_logger() {
    static LOGGER: OnceCell<()> = OnceCell::new();
    LOGGER.get_or_init(|| {
        env_logger::builder()
            .is_test(false)
            .filter_level(LevelFilter::Info)
            .try_init()
            .ok();
    });
}

async fn load_store() -> Arc<XlineStore> {
    let config_path = std::env::var("TEST_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/tests/config.yaml",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    });
    let config = load_config(&config_path).expect("Failed to load config");
    let option = XlineOptions::new(config.xline_config.endpoints.clone());
    Arc::new(XlineStore::new(option).await.expect("connect xline failed"))
}

#[tokio::test]
async fn test_run_dns_server_startup() {
    init_logger();

    let store = load_store().await;

    let file_path = "/home/tcy/project/rk8s/project/rks/tests/test-pod.yaml";
    let pod_yaml = fs::read_to_string(file_path).expect("open error");
    store
        .insert_pod_yaml("test-pod1", &pod_yaml)
        .await
        .expect("insert error");
    let pods = match store.list_pods().await {
        Ok(pods) => pods,
        Err(e) => panic!("Failed to list pods: {:?}", e),
    };

    info!("test get pods: {pods:?}");
    let handle = tokio::spawn(async move {
        let _ = run_dns_server(store, 5300).await;
    });

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl_c");

    handle.abort();
}

#[tokio::test]
async fn test_headless_service_a_records() {
    init_logger();

    let store = load_store().await;

    // create a headless Service (clusterIP: None)
    let svc_yaml = r#"apiVersion: v1
kind: Service
metadata:
    name: test-headless
    namespace: default
spec:
    clusterIP: None
    ports:
        - port: 8080
          name: http
"#;

    store
        .insert_service_yaml("test-headless", svc_yaml)
        .await
        .expect("insert service error");

    // create Endpoints for the headless service with two backend IPs
    let endpoints_yaml = r#"apiVersion: v1
kind: Endpoints
metadata:
    name: test-headless
    namespace: default
subsets:
  - addresses:
      - ip: 10.20.30.3
      - ip: 10.20.30.4
    ports:
      - port: 8080
        name: http
"#;

    store
        .insert_endpoint_yaml("test-headless", endpoints_yaml)
        .await
        .expect("insert endpoints error");

    //query name: test-headless.default.svc.cluster.local.
    let handle = tokio::spawn(async move {
        let _ = run_dns_server(store, 5300).await;
    });

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl_c");

    handle.abort();
}
