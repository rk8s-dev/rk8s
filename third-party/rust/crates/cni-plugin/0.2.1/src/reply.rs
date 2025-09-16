//! Reply types and helpers.

use std::{collections::HashMap, io::stdout, net::IpAddr, path::PathBuf, process::exit};

use ipnetwork::IpNetwork;
use log::debug;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::dns::Dns;
use crate::macaddr::MacAddr;
pub use crate::version::VersionReply;

/// Trait for a reply type to be handled by the [`reply()`] function.
///
/// This is mostly internal, but may be used if you want to output your own
/// reply types for some reason.
pub trait ReplyPayload<'de>: std::fmt::Debug + Serialize + Deserialize<'de> {
	/// The [`exit`] code to be set when replying with this type.
	///
	/// Defaults to 0 (success).
	fn code(&self) -> i32 {
		0
	}
}

/// The reply structure used when returning an error.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReply<'msg> {
	/// The CNI version of the plugin input config.
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,

	/// A code for the error.
	///
	/// Must match the exit code.
	///
	/// Codes 1-99 are reserved by the spec, codes 100+ may be used for plugins'
	/// own error codes. Code 0 is not to be used, as it is for successful exit.
	pub code: i32,

	/// A short message characterising the error.
	///
	/// This is generally a static non-interpolated string.
	pub msg: &'msg str,

	/// A longer message describing the error.
	pub details: String,
}

impl<'de> ReplyPayload<'de> for ErrorReply<'de> {
	/// Sets the exit status of the process to the code of the error reply.
	fn code(&self) -> i32 {
		self.code
	}
}

/// The reply structure used when returning a success.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuccessReply {
	/// The CNI version of the plugin input config.
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,

	/// The list of all interfaces created by this plugin.
	///
	/// If `prev_result` was included in the input config and had interfaces,
	/// they need to be carried on through into this list.
	#[serde(default)]
	pub interfaces: Vec<Interface>,

	/// The list of all IPs assigned by this plugin.
	///
	/// If `prev_result` was included in the input config and had IPs,
	/// they need to be carried on through into this list.
	#[serde(default)]
	pub ips: Vec<Ip>,

	/// The list of all routes created by this plugin.
	///
	/// If `prev_result` was included in the input config and had routes,
	/// they need to be carried on through into this list.
	#[serde(default)]
	pub routes: Vec<Route>,

	/// Final DNS configuration for the namespace.
	pub dns: Dns,

	/// Custom reply fields.
	///
	/// Note that these are off-spec and may be discarded by libcni.
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

impl<'de> ReplyPayload<'de> for SuccessReply {}

impl SuccessReply {
	/// Cast into an abbreviated success reply if the interface list is empty.
	pub fn into_ipam(self) -> Option<IpamSuccessReply> {
		if self.interfaces.is_empty() {
			Some(IpamSuccessReply {
				cni_version: self.cni_version,
				ips: self.ips,
				routes: self.routes,
				dns: self.dns,
				specific: self.specific,
			})
		} else {
			None
		}
	}
}

/// The reply structure used when returning an abbreviated IPAM success.
///
/// It is identical to [`SuccessReply`] except for the lack of the `interfaces`
/// field.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamSuccessReply {
	/// The CNI version of the plugin input config.
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,

	/// The list of all IPs assigned by this plugin.
	///
	/// If `prev_result` was included in the input config and had IPs,
	/// they need to be carried on through into this list.
	#[serde(default)]
	pub ips: Vec<Ip>,

	/// The list of all routes created by this plugin.
	///
	/// If `prev_result` was included in the input config and had routes,
	/// they need to be carried on through into this list.
	#[serde(default)]
	pub routes: Vec<Route>,

	/// Final DNS configuration for the namespace.
	#[serde(default)]
	pub dns: Dns,

	/// Custom reply fields.
	///
	/// Note that these are off-spec and may be discarded by libcni.
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

impl<'de> ReplyPayload<'de> for IpamSuccessReply {}

/// Interface structure for success reply types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
	/// The name of the interface.
	pub name: String,

	/// The hardware address of the interface (if applicable).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mac: Option<MacAddr>,

	/// The path to the namespace the interface is in.
	///
	/// This should be the value passed by `CNI_NETNS`.
	///
	/// If the interface is on the host, this should be set to an empty string.
	pub sandbox: PathBuf,
}

/// IP structure for success reply types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ip {
	/// The IP address.
	pub address: IpNetwork,

	/// The default gateway for this subnet, if one exists.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,

	/// The interface this IP is for.
	///
	/// This must be the index into the `interfaces` list on the parent success
	/// reply structure. It should be `None` for IPAM success replies.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub interface: Option<usize>, // None for ipam
}

/// Route structure for success reply types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
	/// The destination of the route.
	pub dst: IpNetwork,

	/// The next hop address.
	///
	/// If unset, a value in `gateway` in the `ips` array may be used by the
	/// runtime, but this is not mandated and is left to its discretion.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gw: Option<IpAddr>,
}

/// Output the reply as JSON on STDOUT and exit.
pub fn reply<'de, T>(result: T) -> !
where
	T: ReplyPayload<'de>,
{
	debug!("replying with {:#?}", result);
	serde_json::to_writer(stdout(), &result)
		.expect("Error writing result to stdout... chances are you won't get this either");

	exit(result.code());
}
