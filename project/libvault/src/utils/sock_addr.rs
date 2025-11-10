//! This module is a Rust replica of
//! <https://github.com/hashicorp/go-sockaddr/blob/master/sockaddr.go>

use std::{fmt, str::FromStr};

use as_any::AsAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{ip_sock_addr::IpSockAddr, unix_sock_addr::UnixSockAddr};
use crate::errors::RvError;

pub trait CloneBox {
    fn clone_box(&self) -> Box<dyn SockAddr>;
}

impl<T> CloneBox for T
where
    T: 'static + SockAddr + Clone,
{
    fn clone_box(&self) -> Box<dyn SockAddr> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SockAddrType {
    Unknown = 0x0,
    Unix = 0x1,
    IPv4 = 0x2,
    IPv6 = 0x4,
    // IP is the union of IPv4 and IPv6
    IP = 0x6,
}

pub trait SockAddr: Sync + Send + fmt::Display + AsAny + fmt::Debug + CloneBox {
    // contains returns true if the other SockAddr is contained within the receiver
    fn contains(&self, other: &dyn SockAddr) -> bool;

    // equal allows for the comparison of two SockAddrs
    fn equal(&self, other: &dyn SockAddr) -> bool;

    fn sock_addr_type(&self) -> SockAddrType;
}

#[derive(Debug, Clone)]
pub struct SockAddrMarshaler {
    pub sock_addr: Box<dyn SockAddr>,
}

impl SockAddrMarshaler {
    pub fn new(sock_addr: Box<dyn SockAddr>) -> Self {
        SockAddrMarshaler { sock_addr }
    }

    pub fn from_str(s: &str) -> Result<Self, RvError> {
        let sock_addr = new_sock_addr(s)?;
        Ok(SockAddrMarshaler { sock_addr })
    }
}

impl Clone for Box<dyn SockAddr> {
    fn clone(&self) -> Box<dyn SockAddr> {
        self.clone_box()
    }
}

impl fmt::Display for SockAddrMarshaler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.sock_addr)
    }
}

impl PartialEq for SockAddrMarshaler {
    fn eq(&self, other: &Self) -> bool {
        self.sock_addr.equal(&*other.sock_addr)
    }
}

impl Serialize for SockAddrMarshaler {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let sock_addr_str = self.sock_addr.to_string();
        serializer.serialize_str(&sock_addr_str)
    }
}

impl<'de> Deserialize<'de> for SockAddrMarshaler {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let sock_addr = new_sock_addr(&s).map_err(serde::de::Error::custom)?;
        Ok(SockAddrMarshaler { sock_addr })
    }
}

impl fmt::Display for SockAddrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = match self {
            SockAddrType::IPv4 => "IPv4",
            SockAddrType::IPv6 => "IPv6",
            SockAddrType::Unix => "Unix",
            _ => "Unknown",
        };
        write!(f, "{type_str}")
    }
}

impl FromStr for SockAddrType {
    type Err = RvError;
    fn from_str(s: &str) -> Result<Self, RvError> {
        match s {
            "IPv4" | "ipv4" => Ok(SockAddrType::IPv4),
            "IPv6" | "ipv6" => Ok(SockAddrType::IPv6),
            "Unix" | "UNIX" | "unix" => Ok(SockAddrType::Unix),
            _ => Err(RvError::ErrResponse("invalid sockaddr type".to_string())),
        }
    }
}

pub fn new_sock_addr(s: &str) -> Result<Box<dyn SockAddr>, RvError> {
    if let Ok(ip) = IpSockAddr::new(s) {
        return Ok(Box::new(ip));
    }

    if let Ok(ip) = UnixSockAddr::new(s) {
        return Ok(Box::new(ip));
    }

    Err(RvError::ErrResponse(format!(
        "Unable to convert {s} to an IPv4 or IPv6 address, or a UNIX Socket"
    )))
}
