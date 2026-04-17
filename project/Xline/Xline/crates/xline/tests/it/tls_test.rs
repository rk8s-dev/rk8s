use std::{fs, iter, path::PathBuf};

use test_macros::abort_on_panic;
use utils::config::{
    AuthConfig, ClusterConfig, CompactConfig, LogConfig, MetricsConfig, StorageConfig, TlsConfig,
    TraceConfig, XlineServerConfig,
};
use xline_test_utils::{Cluster, enable_auth, set_user};
use xlinerpc::QuicTlsConfig;

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn test_basic_tls() {
    let mut cluster = Cluster::new_with_configs(basic_tls_configs(3)).await;
    cluster.start().await;

    let client = cluster
        .client_with_quic_tls_config(basic_quic_tls_config())
        .await;
    let res = client.kv_client().put("foo", "bar", None).await;
    assert!(res.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn test_mtls() {
    let mut cluster = Cluster::new_with_configs(mtls_configs(3)).await;
    cluster.start().await;

    let client = cluster
        .client_with_quic_tls_config(mtls_quic_tls_config("root"))
        .await;
    let res = client.kv_client().put("foo", "bar", None).await;
    assert!(res.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn test_certificate_authenticate() {
    let mut cluster = Cluster::new_with_configs(mtls_configs(3)).await;
    cluster.start().await;

    let root_client = cluster
        .client_with_quic_tls_config(mtls_quic_tls_config("root"))
        .await;
    enable_auth(&root_client).await.unwrap();

    let u2_client = cluster
        .client_with_quic_tls_config(mtls_quic_tls_config("u2"))
        .await;
    let res = u2_client.kv_client().put("foa", "bar", None).await;
    assert!(res.is_err());
    let u1_client = cluster
        .client_with_quic_tls_config(mtls_quic_tls_config("u1"))
        .await;
    let res = u1_client.kv_client().put("foo", "bar", None).await;
    assert!(res.is_err());

    set_user(&root_client, "u1", "123", "r1", b"foo", &[])
        .await
        .unwrap();
    set_user(&root_client, "u2", "123", "r2", b"foa", &[])
        .await
        .unwrap();

    let res = u2_client.kv_client().put("foa", "bar", None).await;
    assert!(res.is_ok());
    let res = u1_client.kv_client().put("foo", "bar", None).await;
    assert!(res.is_ok());
}

fn configs_with_tls_config(size: usize, tls_config: TlsConfig) -> Vec<XlineServerConfig> {
    iter::repeat(tls_config)
        .map(|tls_config| {
            XlineServerConfig::new(
                ClusterConfig::default(),
                StorageConfig::default(),
                LogConfig::default(),
                TraceConfig::default(),
                AuthConfig::default(),
                CompactConfig::default(),
                tls_config,
                MetricsConfig::default(),
            )
        })
        .take(size)
        .collect()
}

fn basic_quic_tls_config() -> QuicTlsConfig {
    QuicTlsConfig::default().with_peer_ca_cert_pem(fs::read("../../fixtures/ca.crt").unwrap())
}

fn basic_tls_configs(size: usize) -> Vec<XlineServerConfig> {
    configs_with_tls_config(
        size,
        TlsConfig::new(
            None,
            Some(PathBuf::from("../../fixtures/server.crt")),
            Some(PathBuf::from("../../fixtures/server.key")),
            Some(PathBuf::from("../../fixtures/ca.crt")),
            None,
            None,
        ),
    )
}

fn mtls_quic_tls_config(name: &str) -> QuicTlsConfig {
    QuicTlsConfig::default()
        .with_peer_ca_cert_pem(fs::read("../../fixtures/ca.crt").unwrap())
        .with_client_identity_paths(
            PathBuf::from(format!("../../fixtures/{name}_client.crt")),
            PathBuf::from(format!("../../fixtures/{name}_client.key")),
        )
}

fn mtls_configs(size: usize) -> Vec<XlineServerConfig> {
    configs_with_tls_config(
        size,
        TlsConfig::new(
            Some(PathBuf::from("../../fixtures/ca.crt")),
            Some(PathBuf::from("../../fixtures/server.crt")),
            Some(PathBuf::from("../../fixtures/server.key")),
            Some(PathBuf::from("../../fixtures/ca.crt")),
            Some(PathBuf::from("../../fixtures/root_client.crt")),
            Some(PathBuf::from("../../fixtures/root_client.key")),
        ),
    )
}
