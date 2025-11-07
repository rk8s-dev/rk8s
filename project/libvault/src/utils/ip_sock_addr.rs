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
            return Ok(IpSockAddr {
                addr: IpNetwork::from(sock_addr.ip()),
                port: sock_addr.port(),
            });
        } else if let Ok(ip_addr) = IpNetwork::from_str(s) {
            return Ok(IpSockAddr {
                addr: ip_addr,
                port: 0,
            });
        }
        Err(RvError::ErrResponse(format!(
            "Unable to parse {s} to an IP address:"
        )))
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
