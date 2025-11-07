//! This module is a Rust replica of
//! <https://github.com/hashicorp/go-sockaddr/blob/master/unixsock.go>

use std::fmt;

use as_any::Downcast;
use serde::{Deserialize, Serialize};

use super::sock_addr::{SockAddr, SockAddrType};
use crate::errors::RvError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnixSockAddr {
    pub path: String,
}

impl UnixSockAddr {
    pub fn new(s: &str) -> Result<Self, RvError> {
        // Check to make sure the string begins with either a '.' or '/', or contains a '/'.
        if s.len() > 1 && (s[0..1].contains('.') || s[0..1].contains('/') || s.contains('/')) {
            Ok(Self {
                path: s.to_string(),
            })
        } else {
            Err(RvError::ErrResponse(format!(
                "Unable to convert {s} to a UNIX Socke, make sure the string begins with either a '.' or '/', or \
                 contains a '/'"
            )))
        }
    }
}

impl SockAddr for UnixSockAddr {
    fn contains(&self, other: &dyn SockAddr) -> bool {
        if let Some(unix_sock) = other.downcast_ref::<UnixSockAddr>() {
            return self.path == unix_sock.path;
        }

        false
    }

    fn equal(&self, other: &dyn SockAddr) -> bool {
        if let Some(unix_sock) = other.downcast_ref::<UnixSockAddr>() {
            return self.path == unix_sock.path;
        }

        false
    }

    fn sock_addr_type(&self) -> SockAddrType {
        SockAddrType::Unix
    }
}

impl fmt::Display for UnixSockAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}
