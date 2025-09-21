#![allow(unused_imports)]
#![allow(dead_code)]
use hickory_proto::rr::rdata;
use hickory_proto::rr::{Name, RData, Record, RecordType};
use hickory_server::ServerFuture;
use hickory_server::authority::{Authority, AuthorityObject, Catalog, ZoneType};
use hickory_server::store::in_memory::InMemoryAuthority;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;

pub async fn run_dns_server() -> anyhow::Result<()> {
    let origin = Name::from_str("example.local.")?;
    let authority = InMemoryAuthority::empty(origin.clone(), ZoneType::Primary, false);

    let ipv4 = Ipv4Addr::new(10, 0, 0, 1);
    let record = Record::from_rdata(
        origin.clone(),
        3600,
        RData::A(rdata::A(ipv4.octets().into())),
    );

    authority.upsert(record, 0).await;

    let mut catalog = Catalog::new();

    catalog.upsert(
        origin.clone().into(),
        vec![Arc::new(authority) as Arc<dyn AuthorityObject>],
    );

    let mut server = ServerFuture::new(catalog);
    let addr: SocketAddr = "0.0.0.0:5300".parse()?;
    let udp_socket = UdpSocket::bind(addr).await?;
    server.register_socket(udp_socket);

    println!("DNS server listening on {addr}");

    // 6. 启动服务
    server.block_until_done().await?;
    Ok(())
}
