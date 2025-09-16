//! Election example

use etcd_client::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;
    let resp = client.lease_grant(10, None).await?;
    let lease_id = resp.id();
    println!("grant ttl:{:?}, id:{:?}", resp.ttl(), resp.id());

    // campaign
    let resp = client.campaign("myElection", "123", lease_id).await?;
    let leader = resp.leader().unwrap();
    println!(
        "election name:{:?}, leaseId:{:?}",
        leader.name_str(),
        leader.lease()
    );

    // proclaim
    let resp = client
        .proclaim(
            "123",
            Some(ProclaimOptions::new().with_leader(leader.clone())),
        )
        .await?;
    let header = resp.header();
    println!("proclaim header {:?}", header.unwrap());

    // observe
    let mut msg = client.observe(leader.name()).await?;
    loop {
        if let Some(resp) = msg.message().await? {
            println!("observe key {:?}", resp.kv().unwrap().key_str());
            if resp.kv().is_some() {
                break;
            }
        }
    }

    // leader
    let resp = client.leader("myElection").await?;
    let kv = resp.kv().unwrap();
    println!("key is {:?}", kv.key_str());
    println!("value is {:?}", kv.value_str());

    // resign
    let resign_option = ResignOptions::new().with_leader(leader.clone());
    let resp = client.resign(Some(resign_option)).await?;
    let header = resp.header();
    println!("resign header {:?}", header.unwrap());

    Ok(())
}
