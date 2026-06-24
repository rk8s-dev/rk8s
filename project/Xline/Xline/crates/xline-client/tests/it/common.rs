use std::time::Duration;

use tracing::debug;
use utils::config::ClientConfig;
use xline_client::{Client, ClientOptions, error::XlineClientBuildError};
use xline_test_utils::Cluster;

#[inline]
fn it_debug(msg: impl AsRef<str>) {
    debug!("[xline-it] {}", msg.as_ref());
}

pub async fn get_cluster_client() -> Result<(Cluster, Client), XlineClientBuildError> {
    it_debug("get_cluster_client begin");
    let mut cluster = Cluster::new(3).await;
    it_debug(format!(
        "cluster created, client addrs={:?}",
        cluster.all_client_addrs()
    ));
    cluster.start().await;
    it_debug("cluster started");

    let base = ClientConfig::default();
    let client_config = ClientConfig::new(
        *base.wait_synced_timeout(),
        Duration::from_secs(10),
        *base.initial_retry_timeout(),
        *base.max_retry_timeout(),
        *base.retry_count(),
        *base.fixed_backoff(),
        *base.keep_alive_interval(),
    );

    let options = ClientOptions::default().with_client_config(client_config);
    let options =
        options.with_quic_peer_ca_cert(include_bytes!("../../../../fixtures/ca.crt").to_vec());
    it_debug(format!(
        "connecting client with timeout={:?}, addrs={:?}",
        options.client_config().propose_timeout(),
        cluster.all_client_addrs()
    ));
    let client = Client::connect(cluster.all_client_addrs(), options).await?;
    it_debug("client connected");
    Ok((cluster, client))
}
