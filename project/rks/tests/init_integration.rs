// tests/init_integration.rs
use log::info;
use rks::network::init::init_network;
use rks::protocol::config::XlineConfig;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use std::sync::Once;
static INIT: Once = Once::new();

fn init_logging() {
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
