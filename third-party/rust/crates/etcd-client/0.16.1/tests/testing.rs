use etcd_client::{Client, Error};

pub const DEFAULT_TEST_ENDPOINT: &str = "localhost:2379";

pub type Result<T> = std::result::Result<T, Error>;

/// Get client for testing.
pub async fn get_client() -> Result<Client> {
    // Require a leader be present -- with only a single node, this
    // should never fail.
    let options = etcd_client::ConnectOptions::new().with_require_leader(true);
    Client::connect([DEFAULT_TEST_ENDPOINT], Some(options)).await
}
