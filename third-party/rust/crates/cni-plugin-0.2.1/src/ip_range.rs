//! The IpRange type and helpers for IP pools.

use std::net::IpAddr;

use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};

// TODO: enforce all addresses being of the same type
/// A range of IPs, usable for defining an IP pool.
///
/// The subnet is the only required field. The range can be further limited
/// with the `range_start` and `range_end` fields, which are inclusive.
///
/// # Examples
///
/// ```json
/// {"subnet": "10.0.0.0/8"}
/// {"subnet": "10.0.10.0/23", "rangeStart": "10.0.11.0", "rangeEnd": "10.0.11.254"}
/// {"subnet": "192.168.1.1/24", "gateway": "192.168.1.254"}
/// ```
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRange {
	/// The subnet for the range.
	pub subnet: IpNetwork,

	/// The start of the available range within the subnet, inclusive.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub range_start: Option<IpAddr>,

	/// The end of the available range within the subnet, inclusive.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub range_end: Option<IpAddr>,

	/// The gateway of the range.
	///
	/// Interpretation of an absent gateway is left to the implementation.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
}

impl IpRange {
	/// Naive implementation of iterating the IP range.
	///
	/// This iterator will yield every IP available in the range, that is, every
	/// IP in the subnet, except those lower than `range_start`, higher than
	/// `range_end`, or the one which is the `gateway`.
	///
	/// The current implementation iterates through the entire range and filters
	/// off the excluded IPs as per above. For IPv4 this will likely never be an
	/// issue but IPv6 ranges are monstrous and could spend a long time spinning
	/// before reaching `range_start`.
	pub fn iter_free(&self) -> impl Iterator<Item = (IpNetwork, &Self)> {
		let prefix = self.subnet.prefix();
		let range_start = self.range_start;
		let range_end = self.range_end;
		let gateway = self.gateway;

		self.subnet
			.iter()
			.filter(move |ip| {
				if let Some(ref start) = range_start {
					if ip < start {
						// TODO: figure out how to START from there instead
						return false;
					}
				}

				if let Some(ref end) = range_end {
					if ip > end {
						// TODO: figure out how to stop the iterator there instead
						return false;
					}
				}

				if let Some(ref gw) = gateway {
					if ip == gw {
						return false;
					}
				}

				true
			})
			.map(move |ip| (IpNetwork::new(ip, prefix).unwrap(), self))
		// UNWRAP: panics on invalid prefix, but we got it from another IpNetwork
	}
}
