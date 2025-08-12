use rks::api::xlinestore::XlineStore;
use rks::protocol::config::load_config;
use std::sync::Arc;

#[tokio::test]
async fn test_xline_rw() {
    // let config =
    //     load_config("/home/ich/rk8s/project/rks/tests/config.yaml").expect("Failed to load config");
    let config_path = std::env::var("TEST_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/tests/config.yaml",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    });
    let config = load_config(&config_path).expect("Failed to load config");
    let endpoints: Vec<&str> = config.xline_endpoints.iter().map(|s| s.as_str()).collect();
    let store = Arc::new(
        XlineStore::new(&endpoints)
            .await
            .expect("Failed to connect Xline"),
    );

    let _node_list = store.list_nodes().await.expect("List nodes failed");


    // Pod YAML
    let pod_name = "pod-test";
    let pod_yaml = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: pod-test\n";
    store
        .insert_pod_yaml(pod_name, pod_yaml)
        .await
        .expect("Insert pod yaml failed");
    let fetched = store
        .get_pod_yaml(pod_name)
        .await
        .expect("Get pod yaml failed");
    assert_eq!(fetched.as_deref(), Some(pod_yaml));

    let pods = store.list_pods().await.expect("List pods failed");
    assert!(pods.contains(&pod_name.to_string()));
}
