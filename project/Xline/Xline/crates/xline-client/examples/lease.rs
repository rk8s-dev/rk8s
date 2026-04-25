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

    let mut client = Client::connect(curp_members, options).await?.lease_client();

    // grant new lease
    let resp1 = client.grant(60, None).await?;
    let resp2 = client.grant(60, None).await?;
    let lease_id1 = resp1.id;
    let lease_id2 = resp2.id;
    println!("lease id 1: {}", lease_id1);
    println!("lease id 2: {}", lease_id2);

    // get the ttl of lease1
    let resp = client.time_to_live(lease_id1, false).await?;

    println!("remaining ttl: {}", resp.ttl);

    // keep alive lease2
    let (mut keeper, mut stream) = client.keep_alive(lease_id2).await?;

    if let Some(resp) = stream.message().await? {
        println!("new ttl: {}", resp.ttl);
    }

    // keep alive lease2 again using the keeper
    keeper.keep_alive()?;

    // list all leases
    for lease in client.leases().await?.leases {
        println!("lease: {}", lease.id);
    }

    // revoke the leases
    let _resp = client.revoke(lease_id1).await?;
    let _resp = client.revoke(lease_id2).await?;

    Ok(())
}
