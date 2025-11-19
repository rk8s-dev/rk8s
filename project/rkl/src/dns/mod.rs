pub mod authority;
pub mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use hickory_proto::rr::{LowerName, Name};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_server::authority::{AuthorityObject, Catalog};
use hickory_server::server::ServerFuture;
use hickory_server::store::forwarder::ForwardAuthority;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use crate::dns::authority::{LocalAuthority, MemStore};

pub const LOCAL_AUTHORITY_DOMAIN: &str = "rkl.internal.";
pub const LOCAL_NAMESERVER: &str = "172.17.0.1";
pub const PID_FILE_PATH: &str = "/var/run/rkl_dns.pid";
pub const DNS_SOCKET_PATH: &str = "/var/run/rkl_dns.sock";

#[derive(Debug, Deserialize, Serialize)]
pub enum UpdateAction {
    Add,
    Update,
    Delete,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DNSUpdateMessage {
    pub action: UpdateAction,
    pub name: LowerName,
    pub ip: Ipv4Addr,
}

pub fn parse_service_to_domain(srv_name: &str, domain: Option<&str>) -> String {
    match domain {
        Some(network_name) => format!("{srv_name}.{network_name}"),
        None => format!("{srv_name}.{LOCAL_AUTHORITY_DOMAIN}"),
    }
}

/// Start the local DNS Server daemon
///
/// args:
/// - port: the port that server listen (default is 53)
/// - domains: the domains that authority needs to handle LIKE "rkl.local." (for rkl compose each domain is the network name)
///
/// Initialize both local-authority and forward-authority add it to catalog
pub async fn run_local_dns(port: Option<u16>, domains: Vec<LowerName>) -> anyhow::Result<()> {
    let port = port.unwrap_or(53);

    let root_lowername = LowerName::from(Name::root());
    let mem_store = Arc::new(Mutex::new(MemStore::new()));
    let local_authority = LocalAuthority::start(&Name::root().to_string(), mem_store).await?;

    let mut catalog = Catalog::new();

    let local_authority: Arc<dyn AuthorityObject> = local_authority;
    let forwarder = ForwardAuthority::builder(TokioConnectionProvider::default())
        .map_err(|e| anyhow::anyhow!(e))?
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;

    catalog.upsert(
        LowerName::from_str(LOCAL_AUTHORITY_DOMAIN).unwrap(),
        vec![local_authority.clone()],
    );

    for domain in domains {
        catalog.upsert(domain, vec![local_authority.clone()]);
    }

    catalog.upsert(root_lowername.clone(), vec![Arc::new(forwarder)]);

    let mut server = ServerFuture::new(catalog);
    let addr: SocketAddr = format!("{LOCAL_NAMESERVER}:{}", port).parse()?;
    let udp_socket = UdpSocket::bind(addr).await?;
    server.register_socket(udp_socket);

    println!("DNS server listening on {addr}");

    server.block_until_done().await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_run_local_dns_server() {
        run_local_dns(Some(5300), vec![]).await.unwrap()
    }
}
