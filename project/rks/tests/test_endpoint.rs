use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use common::{
    LabelSelector, ObjectMeta, PodSpec, PodStatus, PodTask, ServicePort, ServiceSpec, ServiceTask,
};
use libvault::storage::xline::XlineOptions;
use log::LevelFilter;
use once_cell::sync::OnceCell;
use rks::{
    api::xlinestore::XlineStore,
    controllers::{ControllerManager, endpoint_controller::EndpointController},
    protocol::config::load_config,
};
use serial_test::serial;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio::time::{Duration, Instant};

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

// Helpers adapted from garbage collector tests
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

async fn clean_store(store: &XlineStore) -> Result<()> {
    // delete endpoints
    let endpoints = store.list_endpoints().await?;
    for ep in endpoints {
        let _ = store.delete_endpoint(&ep.metadata.name).await;
    }
    // delete services
    let services = store.list_service_names().await?;
    for name in services {
        let _ = store.delete_service(&name).await;
    }
    // delete pods
    let pods = store.list_pod_names().await?;
    for name in pods {
        let _ = store.delete_pod(&name).await;
    }
    Ok(())
}

async fn setup_endpoint_controller(store: Arc<XlineStore>) -> Result<Arc<ControllerManager>> {
    init_logging();
    let mgr = Arc::new(ControllerManager::new());
    let ctrl = Arc::new(RwLock::new(EndpointController::new(store.clone())));
    mgr.clone().register(ctrl, 1).await?;
    mgr.clone().start_watch(store.clone()).await?;
    // give some time for watch tasks to get ready
    sleep(Duration::from_millis(200)).await;
    Ok(mgr)
}

fn pod_with_ip_and_labels(name: &str, ip: &str, labels: HashMap<String, String>) -> PodTask {
    let meta = ObjectMeta {
        name: name.to_string(),
        ..Default::default()
    };
    let spec = PodSpec {
        node_name: Some("node-1".to_string()),
        containers: vec![],
        init_containers: vec![],
        tolerations: vec![],
    };
    let status = PodStatus {
        pod_ip: Some(ip.to_string()),
        ..Default::default()
    };
    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata: ObjectMeta { labels, ..meta },
        spec,
        status,
    }
}

fn service_with_selector_and_port(
    name: &str,
    selector: Option<LabelSelector>,
    port: i32,
    target_port: Option<i32>,
) -> ServiceTask {
    let meta = ObjectMeta {
        name: name.to_string(),
        ..Default::default()
    };
    let ports = vec![ServicePort {
        port,
        name: Some("http".to_string()),
        target_port,
        protocol: "TCP".to_string(),
        node_port: None,
    }];
    let spec = ServiceSpec {
        service_type: "ClusterIP".to_string(),
        selector,
        ports,
        cluster_ip: Some("10.96.0.1".to_string()),
    };
    ServiceTask {
        api_version: "v1".to_string(),
        kind: "Service".to_string(),
        metadata: meta,
        spec,
    }
}

async fn wait_for_endpoints_ip_and_port(
    store: &XlineStore,
    svc_name: &str,
    expect_ip: &str,
    expect_port: i32,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let eps = store.list_endpoints().await?;
        if let Some(ep) = eps.into_iter().find(|e| e.metadata.name == svc_name) {
            if let Some(sub) = ep.subsets.get(0) {
                let has_ip = sub.addresses.iter().any(|a| a.ip == expect_ip);
                let has_port = sub.ports.iter().any(|p| p.port == expect_port);
                if has_ip && has_port {
                    return Ok(());
                }
            }
        }
        if Instant::now() > deadline {
            anyhow::bail!(
                "timeout waiting for endpoints {} with ip {} and port {}",
                svc_name,
                expect_ip,
                expect_port
            );
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_no_endpoints(store: &XlineStore, svc_name: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let eps = store.list_endpoints().await?;
        let found = eps.into_iter().any(|e| e.metadata.name == svc_name);
        if !found {
            return Ok(());
        }
        if Instant::now() > deadline {
            anyhow::bail!("timeout waiting for no endpoints {}", svc_name);
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn get_endpoints_ips(store: &XlineStore, svc_name: &str) -> Result<Vec<String>> {
    let eps = store.list_endpoints().await?;
    if let Some(ep) = eps.into_iter().find(|e| e.metadata.name == svc_name) {
        if let Some(sub) = ep.subsets.get(0) {
            return Ok(sub.addresses.iter().map(|a| a.ip.clone()).collect());
        }
    }
    Ok(vec![])
}

async fn wait_for_endpoints_ips(
    store: &XlineStore,
    svc_name: &str,
    expect_ips: &[&str],
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let ips = get_endpoints_ips(store, svc_name).await?;
        if expect_ips.iter().all(|ip| ips.iter().any(|x| x == ip)) {
            return Ok(());
        }
        if Instant::now() > deadline {
            anyhow::bail!(
                "timeout waiting for endpoints {} with ips {:?}",
                svc_name,
                expect_ips
            );
        }
        sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[serial]
async fn test_endpoints_created_for_matching_pod() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // Create Service with matchLabels app=demo, port 80 (no targetPort => endpoint port 80)
    let mut match_labels = HashMap::new();
    match_labels.insert("app".to_string(), "demo".to_string());
    let selector = LabelSelector {
        match_labels,
        match_expressions: vec![],
    };
    let svc = service_with_selector_and_port("svc-demo", Some(selector), 80, None);
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    // Start controller after service exists, so it bootstraps caches
    let _mgr = setup_endpoint_controller(store.clone()).await?;

    // Create matching pod with IP
    let mut labels = HashMap::new();
    labels.insert("app".to_string(), "demo".to_string());
    let pod = pod_with_ip_and_labels("pod-1", "10.0.0.1", labels);
    let pod_yaml = serde_yaml::to_string(&pod)?;
    store.insert_pod_yaml(&pod.metadata.name, &pod_yaml).await?;

    // Expect endpoints with address 10.0.0.1 and port 80
    wait_for_endpoints_ip_and_port(&store, "svc-demo", "10.0.0.1", 80, Duration::from_secs(5))
        .await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_empty_selector_matches_all_pods() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // Service with empty selector {} (Some but empty), targetPort 8080 -> endpoint port 8080
    let selector = LabelSelector {
        match_labels: HashMap::new(),
        match_expressions: vec![],
    };
    let svc = service_with_selector_and_port("svc-all", Some(selector), 80, Some(8080));
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    // Any pod should match
    let pod = pod_with_ip_and_labels("pod-any", "10.0.0.2", HashMap::new());
    let pod_yaml = serde_yaml::to_string(&pod)?;
    store.insert_pod_yaml(&pod.metadata.name, &pod_yaml).await?;

    // Expect endpoint port equals targetPort (8080)
    wait_for_endpoints_ip_and_port(&store, "svc-all", "10.0.0.2", 8080, Duration::from_secs(5))
        .await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_external_name_service_is_skipped() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // ExternalName service: controller should skip and never create endpoints
    let spec = ServiceSpec {
        service_type: "ExternalName".to_string(),
        selector: Some(LabelSelector {
            match_labels: HashMap::new(),
            match_expressions: vec![],
        }),
        ports: vec![ServicePort {
            port: 53,
            name: Some("dns".to_string()),
            target_port: None,
            protocol: "TCP".to_string(),
            node_port: None,
        }],
        cluster_ip: None,
    };
    let svc = ServiceTask {
        api_version: "v1".to_string(),
        kind: "Service".to_string(),
        metadata: ObjectMeta {
            name: "svc-ext".to_string(),
            ..Default::default()
        },
        spec,
    };
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    // Create some pod; controller should ignore ExternalName services
    let pod = pod_with_ip_and_labels("pod-x", "10.0.0.3", HashMap::new());
    let pod_yaml = serde_yaml::to_string(&pod)?;
    store.insert_pod_yaml(&pod.metadata.name, &pod_yaml).await?;

    // Verify no endpoints entry for svc-ext
    wait_no_endpoints(&store, "svc-ext", Duration::from_secs(2)).await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_nil_selector_service_is_skipped() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // Nil selector (None) -> controller should skip
    let spec = ServiceSpec {
        service_type: "ClusterIP".to_string(),
        selector: None,
        ports: vec![ServicePort {
            port: 8080,
            name: Some("web".to_string()),
            target_port: None,
            protocol: "TCP".to_string(),
            node_port: None,
        }],
        cluster_ip: Some("10.96.0.10".to_string()),
    };
    let svc = ServiceTask {
        api_version: "v1".to_string(),
        kind: "Service".to_string(),
        metadata: ObjectMeta {
            name: "svc-nil".to_string(),
            ..Default::default()
        },
        spec,
    };
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    // Create some pod; controller should skip due to nil selector
    let pod = pod_with_ip_and_labels("pod-y", "10.0.0.4", HashMap::new());
    let pod_yaml = serde_yaml::to_string(&pod)?;
    store.insert_pod_yaml(&pod.metadata.name, &pod_yaml).await?;

    // Verify no endpoints entry for svc-nil
    wait_no_endpoints(&store, "svc-nil", Duration::from_secs(2)).await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_debounce_pod_burst() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // Service with empty selector {} has debounce 200ms for pod-driven enqueues
    let selector = LabelSelector {
        match_labels: HashMap::new(),
        match_expressions: vec![],
    };
    let svc = service_with_selector_and_port("svc-debounce", Some(selector), 80, Some(8080));
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    // Rapid pod add should not create endpoints before ~200ms
    let pod = pod_with_ip_and_labels("pod-burst", "10.0.1.1", HashMap::new());
    let pod_yaml = serde_yaml::to_string(&pod)?;
    let start = Instant::now();
    store.insert_pod_yaml(&pod.metadata.name, &pod_yaml).await?;

    let early = tokio::time::timeout(
        Duration::from_millis(120),
        wait_for_endpoints_ip_and_port(
            &store,
            "svc-debounce",
            "10.0.1.1",
            8080,
            Duration::from_secs(5),
        ),
    )
    .await;
    assert!(
        early.is_err(),
        "debounce failed: endpoints created too early ({:?})",
        start.elapsed()
    );

    // Should appear after debounce
    wait_for_endpoints_ip_and_port(
        &store,
        "svc-debounce",
        "10.0.1.1",
        8080,
        Duration::from_secs(3),
    )
    .await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_dedupe_coalesce_multiple_pods() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    // Service matches all pods; we add two pods within debounce window
    let selector = LabelSelector {
        match_labels: HashMap::new(),
        match_expressions: vec![],
    };
    let svc = service_with_selector_and_port("svc-dedupe", Some(selector), 80, None);
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    let pod1 = pod_with_ip_and_labels("pod-a", "10.0.2.1", HashMap::new());
    let pod2 = pod_with_ip_and_labels("pod-b", "10.0.2.2", HashMap::new());
    store
        .insert_pod_yaml(&pod1.metadata.name, &serde_yaml::to_string(&pod1)?)
        .await?;
    // insert second within debounce window so both coalesce into one processing
    sleep(Duration::from_millis(60)).await;
    store
        .insert_pod_yaml(&pod2.metadata.name, &serde_yaml::to_string(&pod2)?)
        .await?;

    // Expect both IPs present after one processing cycle
    wait_for_endpoints_ips(
        &store,
        "svc-dedupe",
        &["10.0.2.1", "10.0.2.2"],
        Duration::from_secs(5),
    )
    .await?;

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_update_while_processing_applies_latest() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;
    // Service matches all pods; we add one pod, wait for debounce to expire, then add another pod.
    let selector = LabelSelector {
        match_labels: HashMap::new(),
        match_expressions: vec![],
    };
    let svc = service_with_selector_and_port("svc-dedupe", Some(selector), 80, None);
    let svc_yaml = serde_yaml::to_string(&svc)?;
    store
        .insert_service_yaml(&svc.metadata.name, &svc_yaml)
        .await?;

    let _mgr = setup_endpoint_controller(store.clone()).await?;

    let pod1 = pod_with_ip_and_labels("pod-a", "10.0.2.1", HashMap::new());
    let pod2 = pod_with_ip_and_labels("pod-b", "10.0.2.2", HashMap::new());
    store
        .insert_pod_yaml(&pod1.metadata.name, &serde_yaml::to_string(&pod1)?)
        .await?;
    sleep(Duration::from_millis(200)).await;
    store
        .insert_pod_yaml(&pod2.metadata.name, &serde_yaml::to_string(&pod2)?)
        .await?;

    wait_for_endpoints_ips(
        &store,
        "svc-dedupe",
        &["10.0.2.1", "10.0.2.2"],
        Duration::from_secs(5),
    )
    .await?;

    clean_store(&store).await?;
    Ok(())
}
