//! Lease example.

use etcd_client::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;

    // grant a key
    let resp = client.lease_grant(60, None).await?;
    println!(
        "grant a lease with id {:?}, ttl {:?}",
        resp.id(),
        resp.ttl()
    );
    let id = resp.id();

    // query time to live
    let resp = client.lease_time_to_live(id, None).await?;
    println!(
        "lease({:?}) remain ttl {:?} granted ttl {:?}",
        resp.id(),
        resp.ttl(),
        resp.granted_ttl()
    );

    // keep alive
    let (mut keeper, mut stream) = client.lease_keep_alive(id).await?;
    println!("lease {id:?} keep alive start");
    keeper.keep_alive().await?;
    if let Some(resp) = stream.message().await? {
        println!("lease {:?} keep alive, new ttl {:?}", resp.id(), resp.ttl());
    }

    // get lease list
    let resp = client.leases().await?;
    let lease_status = resp.leases();
    println!("lease status {:?}", lease_status[0].id());

    // revoke a lease
    let _resp = client.lease_revoke(id).await?;
    println!("revoke a lease with id {id:?}");

    // keep alive a revoked lease returns error
    if let Err(err) = client.lease_keep_alive(id).await {
        println!("revoked lease {id:?} keep alive error: {err:?}");
    }
    Ok(())
}
