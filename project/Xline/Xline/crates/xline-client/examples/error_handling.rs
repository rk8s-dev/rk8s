//! An example to show how the errors are organized in `xline-client`
use anyhow::Result;
use xline_client::{Client, ClientOptions, error::XlineClientError, types::kv::PutOptions};
use xlineapi::execute_error::ExecuteError;

#[tokio::main]
async fn main() -> Result<()> {
    // the name and address of all curp members
    // For local testing with 3 QUIC nodes:
    let curp_members = [
        "https://server0:2379", // node0
        "https://server1:2381", // node1
        "https://server2:2383", // node2
    ];

    // Load CA certificate for QUIC TLS verification
    let ca_cert_pem = include_bytes!("../../../fixtures/ca.crt").to_vec();
    let options = ClientOptions::default().with_quic_peer_ca_cert(ca_cert_pem);

    let client = Client::connect(curp_members, options).await?.kv_client();

    // We try to update the key using its previous value.
    // It should return an error and it should be `key not found`
    // as we did not add it before.
    let resp = client
        .put(
            "key",
            "",
            Some(PutOptions::default().with_ignore_value(true)),
        )
        .await;
    let err = resp.unwrap_err();

    // We match the inner error returned by the Curp server.
    // The command should failed at execution stage.
    let XlineClientError::CommandError(ee) = err else {
        unreachable!("the propose error should be an Execute error, but it is {err:?}")
    };

    assert!(
        matches!(ee, ExecuteError::KeyNotFound),
        "the return result should be `KeyNotFound`"
    );
    println!("got error: {ee}");

    Ok(())
}
