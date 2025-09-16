use core::{fmt, str::FromStr};

use crate::parser;

/// MAC address in *EUI-64* format.
#[repr(C)]
#[derive(Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MacAddr8([u8; 8]);

impl MacAddr8 {
    /// Creates a new `MacAddr8` address from the bytes.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF);
    /// ```
    #[allow(clippy::many_single_char_names, clippy::too_many_arguments)]
    pub const fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) -> MacAddr8 {
        MacAddr8([a, b, c, d, e, f, g, h])
    }

    /// Create a new nil `MacAddr8`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::nil();
    /// assert!(addr.is_nil());
    /// ```
    pub const fn nil() -> MacAddr8 {
        MacAddr8([0x00; 8])
    }

    /// Create a new broadcast `MacAddr8`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::broadcast();
    /// assert!(addr.is_broadcast());
    /// ```
    pub const fn broadcast() -> MacAddr8 {
        MacAddr8([0xFF; 8])
    }

    /// Returns `true` if the address is nil.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00);
    ///
    /// assert_eq!(addr.is_nil(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_nil(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    /// Returns `true` if the address is broadcast.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF);
    ///
    /// assert_eq!(addr.is_broadcast(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_broadcast(&self) -> bool {
        self.0.iter().all(|&b| b == 0xFF)
    }

    /// Returns `true` if the address is unicast.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x00, 0x01, 0x44, 0x55, 0x66, 0x77, 0xCD, 0xEF);
    ///
    /// assert_eq!(addr.is_unicast(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn is_unicast(&self) -> bool {
        self.0[0] & 1 == 0
    }

    /// Returns `true` if the address is multicast.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x01, 0x00, 0x0C, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC);
    ///
    /// assert_eq!(addr.is_multicast(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn is_multicast(&self) -> bool {
        self.0[0] & 1 == 1
    }

    /// Returns `true` if the address is universally administered address (UAA).
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x01, 0x00, 0x0C, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC);
    ///
    /// assert_eq!(addr.is_universal(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn is_universal(&self) -> bool {
        self.0[0] & 1 << 1 == 0
    }

    /// Returns `true` if the address is locally administered (LAA).
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0x02, 0x00, 0x0C, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC);
    ///
    /// assert_eq!(addr.is_local(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn is_local(&self) -> bool {
        self.0[0] & 1 << 1 == 2
    }

    /// Converts a `MacAddr8` address to a byte slice.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67, 0x89, 0xAB);
    ///
    /// assert_eq!(addr.as_bytes(), &[0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes a `MacAddr8` address and returns raw bytes.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr8;
    /// let addr = MacAddr8::new(0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67, 0x89, 0xAB);
    ///
    /// assert_eq!(addr.into_array(), [0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    /// ```
    pub const fn into_array(self) -> [u8; 8] {
        self.0
    }
}

impl FromStr for MacAddr8 {
    type Err = parser::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parser::Parser::new(s).read_v8_addr()
    }
}

impl From<[u8; 8]> for MacAddr8 {
    fn from(bytes: [u8; 8]) -> Self {
        MacAddr8(bytes)
    }
}

impl AsRef<[u8]> for MacAddr8 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for MacAddr8 {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// `MacAddr8` can be displayed in different formats.
///
/// # Example
///
/// ```
/// # use macaddr::MacAddr8;
/// let addr = MacAddr8::new(0xab, 0x0d, 0xef, 0x12, 0x34, 0x56, 0x78, 0x9A);
///
/// assert_eq!(&format!("{}",    addr), "AB:0D:EF:12:34:56:78:9A");
/// assert_eq!(&format!("{:-}",  addr), "AB-0D-EF-12-34-56-78-9A");
/// assert_eq!(&format!("{:#}",  addr), "AB0D.EF12.3456.789A");
/// ```
impl fmt::Display for MacAddr8 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.sign_minus() {
            f.write_fmt(format_args!(
                "{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7],
            ))
        } else if f.alternate() {
            f.write_fmt(format_args!(
                "{:02X}{:02X}.{:02X}{:02X}.{:02X}{:02X}.{:02X}{:02X}",
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7],
            ))
        } else {
            f.write_fmt(format_args!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7],
            ))
        }
    }
}
