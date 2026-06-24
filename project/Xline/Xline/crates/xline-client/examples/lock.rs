use anyhow::Result;
use xline_client::{
    Client, ClientOptions,
    clients::Xutex,
    types::kv::{Compare, CompareResult, PutOptions, TxnOp},
};

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

    let client = Client::connect(curp_members, options).await?;

    let lock_client = client.lock_client();
    let kv_client = client.kv_client();

    let mut xutex = Xutex::new(lock_client, "lock-test", None, None).await?;
    // when the `xutex_guard` drop, the lock will be unlocked.
    let xutex_guard = xutex.lock_unsafe().await?;

    let txn_req = xutex_guard
        .txn_check_locked_key()
        .when([Compare::value("key2", CompareResult::Equal, "value2")])
        .and_then([TxnOp::put(
            "key2",
            "value3",
            Some(PutOptions::default().with_prev_kv(true)),
        )])
        .or_else(&[]);

    let _resp = kv_client.txn(txn_req).await?;

    Ok(())
}
