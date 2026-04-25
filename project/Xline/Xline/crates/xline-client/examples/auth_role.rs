use anyhow::Result;
use xline_client::{Client, ClientOptions, types::auth::PermissionType};

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

    // add roles
    client.role_add("role1").await?;
    client.role_add("role2").await?;

    // grant permissions to roles
    client
        .role_grant_permission("role1", PermissionType::Read, "key1", None)
        .await?;
    client
        .role_grant_permission("role2", PermissionType::Readwrite, "key2", None)
        .await?;

    // list all roles and their permissions
    let resp = client.role_list().await?;
    println!("roles:");
    for role in resp.roles {
        println!("{}", role);
        let get_resp = client.role_get(role).await?;
        println!("permmisions:");
        for perm in get_resp.perm {
            println!("{} {}", perm.perm_type, String::from_utf8_lossy(&perm.key));
        }
    }

    // revoke permissions from roles
    client.role_revoke_permission("role1", "key1", None).await?;
    client.role_revoke_permission("role2", "key2", None).await?;

    // delete roles
    client.role_delete("role1").await?;
    client.role_delete("role2").await?;

    Ok(())
}
