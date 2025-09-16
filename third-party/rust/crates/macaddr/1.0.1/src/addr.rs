use core::{fmt, str::FromStr};

use crate::{parser, MacAddr6, MacAddr8, ParseError};

/// A MAC address, either in *EUI-48* or *EUI-64* format.
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum MacAddr {
    V6(MacAddr6),
    V8(MacAddr8),
}

impl MacAddr {
    /// Returns `true` if the address is `MacAddr6` address.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::{MacAddr, MacAddr6};
    /// let addr = MacAddr::from([0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67]);
    ///
    /// assert_eq!(addr.is_v6(), true);
    /// assert_eq!(addr.is_v8(), false);
    /// ```
    pub fn is_v6(&self) -> bool {
        match self {
            MacAddr::V6(_) => true,
            MacAddr::V8(_) => false,
        }
    }

    /// Returns `true` if the address is `MacAddr8` address.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::{MacAddr, MacAddr8};
    /// let addr = MacAddr::from([0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    ///
    /// assert_eq!(addr.is_v6(), false);
    /// assert_eq!(addr.is_v8(), true);
    /// ```
    pub fn is_v8(&self) -> bool {
        match self {
            MacAddr::V6(_) => false,
            MacAddr::V8(_) => true,
        }
    }

    /// Converts a `MacAddr` address to a byte slice.
    ///
    /// Length of the returned slice is depends on the enum member used.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::{MacAddr, MacAddr6};
    /// let addr = MacAddr::from([0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67]);
    ///
    /// assert_eq!(addr.as_bytes(), &[0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67]);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            MacAddr::V6(addr) => addr.as_bytes(),
            MacAddr::V8(addr) => addr.as_bytes(),
        }
    }
}

impl From<MacAddr6> for MacAddr {
    fn from(addr: MacAddr6) -> Self {
        MacAddr::V6(addr)
    }
}

impl From<MacAddr8> for MacAddr {
    fn from(addr: MacAddr8) -> Self {
        MacAddr::V8(addr)
    }
}

impl FromStr for MacAddr {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parser::Parser::new(s).read_addr()
    }
}

impl From<[u8; 6]> for MacAddr {
    fn from(bytes: [u8; 6]) -> Self {
        MacAddr::V6(MacAddr6::from(bytes))
    }
}

impl From<[u8; 8]> for MacAddr {
    fn from(bytes: [u8; 8]) -> Self {
        MacAddr::V8(MacAddr8::from(bytes))
    }
}

impl fmt::Display for MacAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MacAddr::V6(v6) => fmt::Display::fmt(v6, f),
            MacAddr::V8(v8) => fmt::Display::fmt(v8, f),
        }
    }
}
