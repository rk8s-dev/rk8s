use anyhow::Result;
use xline_client::{
    Client, ClientOptions,
    types::kv::{Compare, CompareResult, DeleteRangeOptions, PutOptions, TxnOp, TxnRequest},
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

    let client = Client::connect(curp_members, options).await?.kv_client();

    // put
    client.put("key1", "value1", None).await?;
    client.put("key2", "value2", None).await?;

    // range
    let resp = client.range("key1", None).await?;

    if let Some(kv) = resp.kvs.first() {
        println!(
            "got key: {}, value: {}",
            String::from_utf8_lossy(&kv.key),
            String::from_utf8_lossy(&kv.value)
        );
    }

    // delete
    let resp = client
        .delete(
            "key1",
            Some(DeleteRangeOptions::default().with_prev_kv(true)),
        )
        .await?;

    for kv in resp.prev_kvs {
        println!(
            "deleted key: {}, value: {}",
            String::from_utf8_lossy(&kv.key),
            String::from_utf8_lossy(&kv.value)
        );
    }

    // txn
    let txn_req = TxnRequest::new()
        .when(&[Compare::value("key2", CompareResult::Equal, "value2")][..])
        .and_then(
            &[TxnOp::put(
                "key2",
                "value3",
                Some(PutOptions::default().with_prev_kv(true)),
            )][..],
        )
        .or_else(&[TxnOp::range("key2", None)][..]);

    let _resp = client.txn(txn_req).await?;
    let resp = client.range("key2", None).await?;
    // should print "value3"
    if let Some(kv) = resp.kvs.first() {
        println!(
            "got key: {}, value: {}",
            String::from_utf8_lossy(&kv.key),
            String::from_utf8_lossy(&kv.value)
        );
    }

    // compact
    let rev = resp.header.unwrap().revision;
    let _resp = client.compact(rev, false).await?;

    Ok(())
}
