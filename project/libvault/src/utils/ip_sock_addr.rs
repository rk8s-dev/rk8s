//! This module is a Rust replica of
//! <https://github.com/hashicorp/go-sockaddr/blob/master/ipv4addr.go>

use std::{fmt, net::SocketAddr, str::FromStr};

use as_any::Downcast;
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};

use super::sock_addr::{SockAddr, SockAddrType};
use crate::errors::RvError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpSockAddr {
    pub addr: IpNetwork,
    pub port: u16,
}

impl IpSockAddr {
    pub fn new(s: &str) -> Result<Self, RvError> {
        if let Ok(sock_addr) = SocketAddr::from_str(s) {
            return Ok(IpSockAddr { addr: IpNetwork::from(sock_addr.ip()), port: sock_addr.port() });
        } else if let Ok(ip_addr) = IpNetwork::from_str(s) {
            return Ok(IpSockAddr { addr: ip_addr, port: 0 });
        }
        Err(RvError::ErrResponse(format!("Unable to parse {s} to an IP address:")))
    }
}

impl SockAddr for IpSockAddr {
    fn contains(&self, other: &dyn SockAddr) -> bool {
        if let Some(ip_addr) = other.downcast_ref::<IpSockAddr>() {
            return self.addr.contains(ip_addr.addr.ip());
        }

        false
    }

    fn equal(&self, other: &dyn SockAddr) -> bool {
        if let Some(ip_addr) = other.downcast_ref::<IpSockAddr>() {
            return self.addr == ip_addr.addr && self.port == ip_addr.port;
        }

        false
    }

    fn sock_addr_type(&self) -> SockAddrType {
        match self.addr {
            IpNetwork::V4(_) => SockAddrType::IPv4,
            IpNetwork::V6(_) => SockAddrType::IPv6,
        }
    }
}

impl fmt::Display for IpSockAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.port != 0 {
            return write!(f, "{}:{}", self.addr.ip(), self.port);
        }

        if self.addr.prefix() == 32 {
            return write!(f, "{}", self.addr.ip());
        }

        write!(f, "{}/{}", self.addr.ip(), self.addr.prefix())
    }
}

#[cfg(test)]
mod test {
    use super::{super::sock_addr::SockAddrType, *};

    #[test]
    fn test_ip_sock_addr() {
        let addr1 = IpSockAddr::new("1.1.1.1").unwrap();
        let addr2 = IpSockAddr::new("1.1.1.1").unwrap();
        let addr3 = IpSockAddr::new("2.2.2.2").unwrap();
        let addr4 = IpSockAddr::new("333.333.333.333");
        let addr5 = IpSockAddr::new("1.1.1.1:80").unwrap();
        let addr6 = IpSockAddr::new("1.1.1.1:80").unwrap();
        let addr7 = IpSockAddr::new("1.1.1.1:8080").unwrap();
        let addr8 = IpSockAddr::new("2.2.2.2:80").unwrap();
        let addr9 = IpSockAddr::new("192.168.0.0/16").unwrap();
        let addr10 = IpSockAddr::new("192.168.0.0/24").unwrap();
        let addr11 = IpSockAddr::new("192.168.0.1").unwrap();
        let addr12 = IpSockAddr::new("192.168.1.1").unwrap();

        assert!(addr4.is_err());
        assert!(addr1.contains(&addr2));
        assert!(addr1.equal(&addr2));
        assert!(!addr1.contains(&addr3));
        assert!(!addr1.equal(&addr3));
        assert_eq!(addr1.sock_addr_type(), SockAddrType::IPv4);
        assert!(addr5.contains(&addr6));
        assert!(addr5.equal(&addr6));
        assert!(!addr5.equal(&addr7));
        assert!(!addr5.equal(&addr8));
        assert!(addr9.contains(&addr10));
        assert!(addr9.contains(&addr11));
        assert!(addr9.contains(&addr12));
        assert!(!addr9.contains(&addr1));
        assert!(addr10.contains(&addr9));
        assert!(addr10.contains(&addr11));
        assert!(!addr10.contains(&addr12));
        assert!(!addr9.equal(&addr10));
        assert!(!addr9.equal(&addr11));
    }
}
