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

    // add user
    client.user_add("user1", "", true).await?;
    client.user_add("user2", "", true).await?;

    // change user1's password to "123"
    client.user_change_password("user1", "123").await?;

    // grant roles
    client.user_grant_role("user1", "role1").await?;
    client.user_grant_role("user2", "role2").await?;

    // list all users and their roles
    let resp = client.user_list().await?;
    for user in resp.users {
        println!("user: {}", user);
        let get_resp = client.user_get(user).await?;
        println!("roles:");
        for role in get_resp.roles.iter() {
            print!("{} ", role);
        }
        println!();
    }

    // revoke role from user
    client.user_revoke_role("user1", "role1").await?;
    client.user_revoke_role("user2", "role2").await?;

    // delete users
    client.user_delete("user1").await?;
    client.user_delete("user2").await?;

    Ok(())
}
