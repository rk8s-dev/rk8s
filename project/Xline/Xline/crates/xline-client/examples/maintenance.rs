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

    let mut client = Client::connect(curp_members, options)
        .await?
        .maintenance_client();

    // snapshot
    let mut msg = client.snapshot().await?;
    let mut snapshot = vec![];
    loop {
        if let Some(resp) = msg.message().await? {
            snapshot.extend_from_slice(&resp.blob);
            if resp.remaining_bytes == 0 {
                break;
            }
        }
    }
    println!("snapshot size: {}", snapshot.len());

    Ok(())
}
