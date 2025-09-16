use core::{fmt, str::FromStr};

use crate::parser;

/// MAC address in *EUI-48* format.
#[repr(C)]
#[derive(Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MacAddr6([u8; 6]);

impl MacAddr6 {
    /// Creates a new `MacAddr6` address from the bytes.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x01, 0x23, 0x45, 0x67, 0x89, 0xAB);
    /// ```
    #[allow(clippy::many_single_char_names)]
    pub const fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> MacAddr6 {
        MacAddr6([a, b, c, d, e, f])
    }

    /// Create a new nil `MacAddr6`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::nil();
    /// assert!(addr.is_nil());
    /// ```
    pub const fn nil() -> MacAddr6 {
        MacAddr6([0x00; 6])
    }

    /// Create a new broadcast `MacAddr6`.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::broadcast();
    /// assert!(addr.is_broadcast());
    /// ```
    pub const fn broadcast() -> MacAddr6 {
        MacAddr6([0xFF; 6])
    }

    /// Returns `true` if the address is nil.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x00, 0x00, 0x00, 0x00, 0x00, 0x00);
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
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF);
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
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x00, 0x01, 0x44, 0x55, 0x66, 0x77);
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
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x01, 0x00, 0x0C, 0xCC, 0xCC, 0xCC);
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
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x01, 0x00, 0x0C, 0xCC, 0xCC, 0xCC);
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
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0x02, 0x00, 0x0C, 0xCC, 0xCC, 0xCC);
    ///
    /// assert_eq!(addr.is_local(), true);
    /// ```
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn is_local(&self) -> bool {
        self.0[0] & 1 << 1 == 2
    }

    /// Converts a `MacAddr6` address to a byte slice.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67);
    ///
    /// assert_eq!(addr.as_bytes(), &[0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67]);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes `MacAddr6` address and returns raw bytes array.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use macaddr::MacAddr6;
    /// let addr = MacAddr6::new(0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67);
    ///
    /// assert_eq!(addr.into_array(), [0xAC, 0xDE, 0x48, 0x23, 0x45, 0x67]);
    /// ```
    pub const fn into_array(self) -> [u8; 6] {
        self.0
    }
}

impl FromStr for MacAddr6 {
    type Err = parser::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parser::Parser::new(s).read_v6_addr()
    }
}

impl From<[u8; 6]> for MacAddr6 {
    fn from(bytes: [u8; 6]) -> Self {
        MacAddr6(bytes)
    }
}

impl AsRef<[u8]> for MacAddr6 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for MacAddr6 {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// `MacAddr6` can be displayed in different formats.
///
/// # Example
///
/// ```
/// # use macaddr::MacAddr6;
/// let addr = MacAddr6::new(0xab, 0x0d, 0xef, 0x12, 0x34, 0x56);
///
/// assert_eq!(&format!("{}",    addr), "AB:0D:EF:12:34:56");
/// assert_eq!(&format!("{:-}",  addr), "AB-0D-EF-12-34-56");
/// assert_eq!(&format!("{:#}",  addr), "AB0.DEF.123.456");
/// ```
impl fmt::Display for MacAddr6 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.sign_minus() {
            f.write_fmt(format_args!(
                "{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5],
            ))
        } else if f.alternate() {
            let p1 = u16::from(self.0[0]) * 16 + u16::from(self.0[1] / 16);
            let p2 = u16::from(self.0[1] % 16) * 256 + u16::from(self.0[2]);
            let p3 = u16::from(self.0[3]) * 16 + u16::from(self.0[4] / 16);
            let p4 = u16::from(self.0[4] % 16) * 256 + u16::from(self.0[5]);

            f.write_fmt(format_args!("{:03X}.{:03X}.{:03X}.{:03X}", p1, p2, p3, p4,))
        } else {
            f.write_fmt(format_args!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5],
            ))
        }
    }
}
