//! Auth role example.

use etcd_client::*;

#[allow(unused_must_use)]
#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;
    let role1 = "role1";
    let role2 = "role2";

    // ignore result
    client.role_delete(role1).await;
    client.role_delete(role2).await;

    client.role_add(role1).await?;
    println!("add role1 successfully");

    client.role_add(role2).await?;
    println!("add role2 successfully");

    let perm = Permission::read("456").with_range_end("457");
    client.role_grant_permission(role1, perm).await?;
    println!("grant role1 permission successfully");

    let perm1 = Permission::read("abc");
    client.role_grant_permission(role1, perm1).await?;
    println!("grant role1 permission successfully");

    let perm2 = Permission::read_write("123").with_prefix();
    client.role_grant_permission(role1, perm2).await?;
    println!("grant role1 permission successfully");

    let perm3 = Permission::read("a");
    client.role_grant_permission(role1, perm3).await?;
    println!("grant role1 permission successfully");

    let perm4 = Permission::write("b").with_prefix();
    let resp = client.role_grant_permission(role1, perm4).await?;
    println!("{:?}", resp.header());

    let perm5 = Permission::read_write("d").with_range_end("h");
    let resp = client.role_grant_permission(role1, perm5).await?;
    println!("{:?}", resp.header());

    let resp = client.role_get(role1).await?;
    //show the result
    println!("Role {role1:?}\n{resp:?}");

    let resp = client.role_revoke_permission(role1, "abc", None).await?;
    println!("{:?}", resp.header());

    let resp = client
        .role_revoke_permission(
            role1,
            "456",
            Some(RoleRevokePermissionOptions::new().with_range_end("457")),
        )
        .await?;
    println!("{:?}", resp.header());

    let resp = client
        .role_revoke_permission(
            role1,
            "b",
            Some(RoleRevokePermissionOptions::new().with_prefix()),
        )
        .await?;
    println!("{:?}", resp.header());

    let resp = client.role_get(role1).await?;
    //show the result
    println!("Role {role1:?}\n{resp:?}");

    let resp = client.role_list().await?;
    //show the result
    println!("Roles: \n {:?} ", resp.roles());

    client.role_delete(role1).await?;
    println!("delete role1 successfully",);
    client.role_delete(role2).await?;
    println!("delete role2 successfully",);

    let resp = client.role_list().await?;
    //show the result
    println!("Roles: \n {:?} ", resp.roles());

    Ok(())
}
