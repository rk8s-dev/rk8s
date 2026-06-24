use anyhow::Result;
use xline_client::{Client, ClientOptions};

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
    let mut watch_client = client.watch_client();
    let kv_client = client.kv_client();

    // watch
    let (mut watcher, mut stream) = watch_client.watch("key1", None).await?;
    kv_client.put("key1", "value1", None).await?;

    let resp = stream.message().await?.unwrap();
    let kv = resp.events[0].kv.as_ref().unwrap();

    println!(
        "got key: {}, value: {}",
        String::from_utf8_lossy(&kv.key),
        String::from_utf8_lossy(&kv.value)
    );

    // cancel the watch
    watcher.cancel()?;

    Ok(())
}
