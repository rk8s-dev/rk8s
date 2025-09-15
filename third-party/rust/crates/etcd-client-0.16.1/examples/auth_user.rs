//! Auth user example.

use etcd_client::*;

#[allow(unused_must_use)]
#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;
    let name1 = "usr1";
    let password1 = "pwd1";
    let name2 = "usr2";
    let password2 = "pwd2";
    let role1 = "role1";
    let role2 = "role2";

    // ignore result
    client.user_delete(name1).await;
    client.user_delete(name2).await;
    client.role_delete(role1).await;
    client.role_delete(role2).await;

    client.user_add(name1, password1, None).await?;
    println!("add usr1 successfully");

    client
        .user_add(name2, password2, Some(UserAddOptions::new().with_no_pwd()))
        .await?;
    println!("add usr2 successfully");

    client.user_change_password(name1, password2).await?;
    println!("change password for usr1 successfully");

    client.role_add(role1).await?;
    client.role_add(role2).await?;
    client.user_grant_role(name1, role1).await?;
    println!("grant role1 to usr1 successfully");

    client.user_grant_role(name1, role2).await?;
    println!("grant role2 to usr1 successfully");

    let resp = client.user_get(name1).await?;
    //show the result
    println!("User {name1:?}\n{resp:?}");

    client.user_revoke_role(name1, role1).await?;
    println!("revoke role1 from usr1 successfully");

    let resp = client.user_get(name1).await?;
    //show the result
    println!("User {name1:?}\n{resp:?}");

    let resp = client.user_list().await?;
    //show the result
    println!("Users: \n {:?} ", resp.users());

    client.user_delete(name1).await?;
    println!("delete usr1 successfully",);
    client.user_delete(name2).await?;
    println!("delete usr2 successfully",);

    let resp = client.user_list().await?;
    //show the result
    println!("Users: \n {:?} ", resp.users());

    Ok(())
}
