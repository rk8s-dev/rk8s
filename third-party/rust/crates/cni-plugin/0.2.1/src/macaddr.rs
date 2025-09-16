//! MAC address (de)serialisation.

use std::{fmt, str::FromStr};

use macaddr::{MacAddr6, ParseError};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

/// A MAC address.
///
/// This type’s entire purpose is to serialize and deserialize from the string
/// representation of a MAC address, rather than `[u8; 6]` as the underlying
/// type does.
#[derive(Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub struct MacAddr(pub MacAddr6);

impl From<MacAddr6> for MacAddr {
	fn from(m: MacAddr6) -> Self {
		Self(m)
	}
}

impl From<MacAddr> for MacAddr6 {
	fn from(m: MacAddr) -> Self {
		m.0
	}
}

impl fmt::Display for MacAddr {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl FromStr for MacAddr {
	type Err = ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		MacAddr6::from_str(s).map(Self)
	}
}

impl Serialize for MacAddr {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		self.to_string().serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for MacAddr {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let j = String::deserialize(deserializer)?;
		Self::from_str(&j).map_err(Error::custom)
	}
}
