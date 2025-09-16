//! Cluster example.

use etcd_client::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;

    // add a member
    let resp = client.member_add(["localhost:2520"], None).await?;
    println!("add one member with cluster is as {:?}", resp.member());
    let member = resp.member();
    let id = member.unwrap().id();

    // promote a learner
    let resp = client.member_promote(id).await?;
    println!("member list with current cluster is {:?}", resp.members());

    // list all members
    let resp = client.member_list().await?;
    let member_list = resp.members();
    println!(
        "member id is {:?}, url is {:?}",
        member_list[0].id(),
        member_list[0].peer_urls()
    );
    println!(
        "member id is {:?}, url is {:?}",
        member_list[1].id(),
        member_list[1].peer_urls()
    );
    println!(
        "member id is {:?}, url is {:?}",
        member_list[2].id(),
        member_list[2].peer_urls()
    );

    // remove the added member
    let _resp = client.member_remove(id).await?;
    println!("remove a member with id {id:?}");
    Ok(())
}
