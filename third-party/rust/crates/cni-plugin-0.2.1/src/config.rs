//! Configuration structures.
//!
//! You’ll want to start with [`NetworkConfig`].

use std::collections::HashMap;

use ipnetwork::IpNetwork;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ip_range::IpRange, macaddr::MacAddr};

pub use crate::dns::Dns;

/// Top-level network configuration.
///
/// This is the structure that is provided to plugins by CNI, not the structure
/// that administrators write to configure CNI. As such, some fields defined in
/// the spec to only exist in the administrative schema are not included here.
///
/// This struct’s members include all fields described by the spec, as well as a
/// `specific` field which is a map of [`String`]s to [`Value`]s, and will catch
/// any custom fields present at the top level of the configuration. There are
/// other `specific` fields in _some_ of the structs that are in fields below.
///
/// In general, this structure will only ever be read or modified by a plugin,
/// but all fields are public to allow construction if necessary.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
	/// Version of the CNI spec to which this configuration conforms.
	///
	/// This is a [Semantic Version 2.0](https://semver.org/) version number,
	/// and that is enforced here by being a [`Version`], not a string.
	///
	/// This version must be used when creating [replies][crate::reply], which
	/// include a similar field. The spec does not currently cover the case
	/// where an [`ErrorReply`][crate::reply::ErrorReply] must be created
	/// _before_ the config is parsed, or in cases of unparseable config; this
	/// is [under discussion](https://github.com/containernetworking/cni/issues/827).
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,

	/// Name of the network configuration.
	///
	/// This is unique across all network configurations on a host (or other
	/// administrative domain). There are format restrictions but as this field
	/// will always be provided by the CNI runtime and is not to be created or
	/// altered by plugins, those are not checked here.
	pub name: String,

	/// Name of the top-level plugin binary on disk.
	///
	/// This is called `type` in the JSON.
	///
	/// The “top-level” distinction is because a config may include an IPAM
	/// section, which contains its own `plugin` field, and this full
	/// configuration is provided to all sub plugins (via delegation), so a
	/// plugin may receive a configuration where this field doesn’t match its
	/// own name.
	#[serde(rename = "type")]
	pub plugin: String,

	/// Arbitrary arguments passed by the runtime.
	///
	/// This is a map of arbitrary arguments passed by the runtime, which might
	/// be on their own or on behalf of the user/operator. Plugins are free to
	/// ignore it if they’re not expecting anything within.
	///
	/// This replaces the older and deprecated `CNI_ARGS` environment variable,
	/// which this library doesn’t read (you may do so yourself if needed).
	#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	pub args: HashMap<String, Value>,

	/// Set up an IP masquerade on the host for this network.
	///
	/// This is an optional, “well-known” configuration field.
	///
	/// If `true`, and if the plugin supports it, an IP masquerade must be set
	/// up on the host for this network.
	#[serde(default)]
	pub ip_masq: bool,

	/// IP Address Management sub-config.
	///
	/// This is an optional, “well-known” configuration field.
	///
	/// If present, and if the plugin supports it, the IPAM plugin specified by
	/// the `plugin` field of [`IpamConfig`] must be invoked via delegation.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ipam: Option<IpamConfig>,

	/// DNS sub-config.
	///
	/// This is an optional, “well-known” configuration field.
	///
	/// If present, and if the plugin supports it, the DNS settings specified
	/// must be configured for this network.
	///
	/// Note that this section is sourced from the administrative configuration.
	/// There is another field for runtime-provided DNS settings when supported,
	/// see [`RuntimeConfig`].
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub dns: Option<Dns>,

	/// Dynamic information provided by the runtime.
	///
	/// This is an optional, “well-known” configuration field, named
	/// `runtimeConfig` in the spec, which is derived from the `capabilities`
	/// field only present in the administrative configuration.
	///
	/// Plugins can request that the runtime insert this dynamic configuration
	/// by explicitly listing their capabilities in the administrative
	/// configuration. Unlike the `args` field, plugins are expected to act on
	/// the data provided, and should not ignore it if they can’t.
	#[serde(
		default,
		rename = "runtimeConfig",
		skip_serializing_if = "Option::is_none"
	)]
	pub runtime: Option<RuntimeConfig>,

	/// The result of the previous plugin in a chain.
	///
	/// This is the `prevResult` field in the spec.
	///
	/// This field may contain anything, but most likely contains a
	/// [`SuccessReply`][crate::reply::SuccessReply] or
	/// [`IpamSuccessReply`][crate::reply::IpamSuccessReply]. You should use
	/// [`serde_json::from_value`] to reinterpret it as whatever you expect it
	/// to be.
	///
	/// Plugins provided a `prev_result` as part of their input configuration
	/// must per spec output it as their result, with any possible modifications
	/// made by that plugin included. If a plugin makes no changes that would be
	/// reflected in the success reply, then it must output a reply equivalent
	/// to the provided `prev_result`.
	///
	/// In a `CHECK` operation, the plugin must consult the `prev_result` to
	/// determine the expected interfaces and addresses.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub prev_result: Option<Value>,

	/// Custom top-level fields.
	///
	/// This is a [`serde(flatten)`](https://serde.rs/field-attrs.html#flatten)
	/// field which aggregates any and all additional custom fields not covered
	/// above.
	///
	/// Plugins may use this for custom configuration.
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

/// IP Address Management configuration.
///
/// IPAM plugins will be invoked with the full [`NetworkConfig`] as input, but
/// should take their configuration from this section only.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamConfig {
	/// Name of the IPAM plugin binary on disk.
	///
	/// This is called `type` in the JSON.
	#[serde(rename = "type")]
	pub plugin: String,

	/// All other IPAM fields.
	///
	/// This is a [`serde(flatten)`](https://serde.rs/field-attrs.html#flatten)
	/// field which aggregates any and all additional fields apart from the
	/// `plugin` field above.
	///
	/// The spec describes nothing in particular for this section, so it is
	/// entirely up to plugins to interpret it as required.
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

/// Dynamic information provided by the runtime.
///
/// These are generated by the runtime. Note that not all runtimes implement all
/// of these. Also note that all fields below except for `specific` are for
/// “well-known” configs as documented in [CONVENTIONS.md], and those that are
/// not implemented here will appear in the `specific` map.
///
/// Finally, note this struct is marked non-exhaustive: new fields may be added
/// to hoist new “well-known” configs out of the `specific` map.
///
/// [CONVENTIONS.md]: https://github.com/containernetworking/cni/blob/master/CONVENTIONS.md
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RuntimeConfig {
	/// List of port mappings from host to namespace to set up.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub port_mappings: Vec<PortMapping>,

	/// List of pools to use for IPAM.
	///
	/// An IP pool is a list of IP ranges, hence this is this a list of lists of
	/// IP ranges. The outer list defines how many IP addresses to allocate,
	/// with each inner pool defining where to allocate from.
	///
	/// The [`IpRange`] type has methods to help with allocation.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub ips_ranges: Vec<Vec<IpRange>>,

	/// Bandwidth limits to set.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub bandwidth: Option<BandwidthLimits>,

	/// DNS configuration.
	///
	/// Note that this section is set by the runtime. There is another field for
	/// DNS in sourced in the administrative config, see [`NetworkConfig`].
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub dns: Option<Dns>,

	/// List of static IPs to use for IPAM.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub ips: Vec<IpNetwork>,

	/// MAC address to use for the interface.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mac: Option<MacAddr>,

	/// List of names mapped to the IPs assigned to this interface.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub aliases: Vec<String>,

	// TODO: infinibandGUID (behind feature)
	// TODO: (PCI) deviceID (behind feature)
	/// Custom runtime fields.
	///
	/// This is a [`serde(flatten)`](https://serde.rs/field-attrs.html#flatten)
	/// field which aggregates any and all additional custom fields not covered
	/// above.
	///
	/// Take note of the caveats in the struct documentation.
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

/// Port mapping entry.
///
/// This defines a single mapping (forwarding) of a port from the host to the
/// container namespace.
///
/// It is up to the implementation what to do if the `protocol` is left `None`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortMapping {
	/// Port on the host.
	pub host_port: u16,

	/// Port in the namespace.
	pub container_port: u16,

	/// Protocol to forward.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub protocol: Option<PortProtocol>,
}

/// Protocol for a port.
///
/// This is non-exhaustive as more protocols may be added.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PortProtocol {
	/// The TCP protocol.
	Tcp,

	/// The UDP protocol.
	Udp,
}

/// Bandwidth limits to set on the interface.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthLimits {
	/// Rate limit for incoming traffic in bits per second.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ingress_rate: Option<usize>,

	/// Burst limit for incoming traffic in bits.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ingress_burst: Option<usize>,

	/// Rate limit for outgoing traffic in bits per second.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub egress_rate: Option<usize>,

	/// Burst limit for outgoing traffic in bits.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub egress_burst: Option<usize>,
}
