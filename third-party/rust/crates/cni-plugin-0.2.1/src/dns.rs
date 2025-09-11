use std::net::IpAddr;

use serde::{Deserialize, Serialize};

/// DNS configuration or settings.
///
/// Some plugins may make use of this. While the schema is set, it is not a part
/// of the spec formally, and plugins are only required to respect their
/// intended semantics if they care about these.
///
/// All fields are optional ([`Vec`]s will default to empty).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dns {
	/// List of DNS nameservers this network is aware of.
	///
	/// The list is priority-ordered.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub nameservers: Vec<IpAddr>,

	/// The local domain used for short hostname lookups.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub domain: Option<String>,

	/// List of search domains for short hostname lookups.
	///
	/// This effectively supersedes the `domain` field and will be preferred
	/// over it by most resolvers.
	///
	/// The list is priority-ordered.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub search: Vec<String>,

	/// List of options to be passed to the resolver.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub options: Vec<String>,
}
