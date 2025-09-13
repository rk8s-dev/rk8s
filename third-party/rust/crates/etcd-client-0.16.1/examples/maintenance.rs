//! Maintenance example

use etcd_client::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;

    // Get alarm.
    let resp = client
        .alarm(AlarmAction::Get, AlarmType::None, Some(AlarmOptions::new()))
        .await?;
    let mems = resp.alarms();
    println!("{} members have alarm.", mems.len());

    // Get status.
    let resp = client.status().await?;
    println!("version: {}, db_size: {}", resp.version(), resp.db_size());

    // Defragment.
    let _resp = client.defragment().await?;

    // Get hash value.
    let resp = client.hash().await?;
    println!("hash: {}", resp.hash());

    // Get hash key value.
    let resp = client.hash_kv(0).await?;
    println!(
        "hash: {}, revision: {}",
        resp.hash(),
        resp.compact_version()
    );

    // Get snapshot.
    let mut msg = client.snapshot().await?;
    loop {
        let resp = msg.message().await?;
        if let Some(r) = resp {
            println!("Receive blob len {}", r.blob().len());
            if r.remaining_bytes() == 0 {
                break;
            }
        }
    }
    let resp = msg.message().await?;
    if let Some(r) = resp {
        // snapshot hash is sent after r.remaining_bytes() == 0
        println!(
            "Received snapshot hash {:?} of len {}",
            r.blob(),
            r.blob().len()
        );
    }

    // Mover leader
    let resp = client.member_list().await?;
    let member_list = resp.members();

    let resp = client.status().await?;
    let leader_id = resp.leader();
    println!("status {:?}, leader_id {:?}", resp, resp.leader());

    let mut member_id = leader_id;
    for member in member_list {
        if member.id() != leader_id {
            member_id = member.id();
            println!("member_id {:?}, name is {:?}", member.id(), member.name());
            break;
        }
    }

    let resp = client.move_leader(member_id).await?;
    let header = resp.header();
    if member_id == leader_id {
        assert!(header.is_none());
    } else {
        println!("move_leader header {header:?}");
    }

    Ok(())
}
