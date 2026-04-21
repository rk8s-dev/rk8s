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

    let client = Client::connect(curp_members, options).await?.auth_client();

    // enable auth
    let _resp = client.auth_enable().await?;

    // connect using the root user
    let ca_cert_pem2 = include_bytes!("../../../fixtures/ca.crt").to_vec();
    let options = ClientOptions::default()
        .with_user("root", "rootpwd")
        .with_quic_peer_ca_cert(ca_cert_pem2);
    let client = Client::connect(curp_members, options).await?.auth_client();

    // disable auth
    let _resp = client.auth_disable();

    // get auth status
    let resp = client.auth_status().await?;
    println!("auth status:");
    println!(
        "enabled: {}, revision: {}",
        resp.enabled, resp.auth_revision
    );

    Ok(())
}
