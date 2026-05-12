use std::{error::Error, time::Duration};

use test_macros::abort_on_panic;
use tokio::{net::TcpListener, time::sleep};
use xline_client::{Client, ClientOptions};
use xline_test_utils::Cluster;

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn xline_remove_node() -> Result<(), Box<dyn Error>> {
    let mut cluster = Cluster::new(3).await;
    cluster.start().await;
    let mut cluster_client = Client::connect(
        cluster.all_client_addrs(),
        ClientOptions::default().with_quic_tls_config(Cluster::create_quic_tls_config()),
    )
    .await?
    .cluster_client();
    let list_res = cluster_client.member_list(false).await?;
    assert_eq!(list_res.members.len(), 3);
    let remove_id = list_res.members[0].id;
    let remove_res = cluster_client.member_remove(remove_id).await?;
    assert_eq!(remove_res.members.len(), 2);
    assert!(remove_res.members.iter().all(|m| m.id != remove_id));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn xline_add_node() -> Result<(), Box<dyn Error>> {
    let mut cluster = Cluster::new(3).await;
    cluster.start().await;
    let new_node_host = "localhost";
    let client = Client::connect(
        cluster.all_client_addrs(),
        ClientOptions::default().with_quic_tls_config(Cluster::create_quic_tls_config()),
    )
    .await?;
    let mut cluster_client = client.cluster_client();
    let kv_client = client.kv_client();
    _ = kv_client.put("key", "value", None).await?;
    let new_node_peer_listener = TcpListener::bind("127.0.0.1:0").await?;
    let new_node_peer_urls = vec![format!(
        "https://{new_node_host}:{}",
        new_node_peer_listener.local_addr()?.port()
    )];
    let new_node_client_listener = TcpListener::bind("127.0.0.1:0").await?;
    let add_res = cluster_client.member_add(new_node_peer_urls, false).await?;
    assert_eq!(add_res.members.len(), 4);
    cluster
        .run_node(new_node_client_listener, new_node_peer_listener)
        .await;
    let res = kv_client.range("key", None).await?;
    assert_eq!(res.kvs[0].value, b"value");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
async fn xline_update_node() -> Result<(), Box<dyn Error>> {
    let mut cluster = Cluster::new(3).await;
    cluster.start().await;
    let mut cluster_client = cluster.client().await.cluster_client();
    let old_list_res = cluster_client.member_list(false).await?;
    assert_eq!(old_list_res.members.len(), 3);
    let update_id = old_list_res.members[0].id;
    let port = old_list_res.members[0]
        .peer_ur_ls
        .first()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let update_res = cluster_client
        .member_update(update_id, [format!("http://localhost:{}", port)])
        .await?;
    assert_eq!(update_res.members.len(), 3);
    sleep(Duration::from_secs(3)).await;
    let new_list_res = cluster_client.member_list(false).await?;
    assert_eq!(new_list_res.members.len(), 3);
    let old_addr = &old_list_res
        .members
        .iter()
        .find(|m| m.id == update_id)
        .unwrap()
        .peer_ur_ls;
    let new_addr = &new_list_res
        .members
        .iter()
        .find(|m| m.id == update_id)
        .unwrap()
        .peer_ur_ls;
    assert_ne!(old_addr, new_addr);

    Ok(())
}
