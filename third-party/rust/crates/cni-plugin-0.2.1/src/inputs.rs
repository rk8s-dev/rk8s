use std::path::PathBuf;

use crate::{config::NetworkConfig, Cni, Command};

/// An alternate representation of plugin inputs.
///
/// This can be obtained from [`Cni`] with the [`Cni::into_inputs()`] method.
#[derive(Clone, Debug)]
pub struct Inputs {
	/// The command given to the plugin.
	pub command: Command,

	/// The container ID, as provided by the runtime.
	///
	/// The spec says:
	/// > A unique plaintext identifier for a container, allocated by the
	/// > runtime. Not empty.
	///
	/// In practice, this may not be the ID of an actual container, but
	/// rather the ID of the logical container grouping this network applies
	/// to. E.g. a Pod, Alloc, etc.
	pub container_id: String,

	/// The name of the interface to create, delete, check, or manage inside the container.
	pub ifname: String,

	/// The container’s “isolation domain.”
	///
	/// If using network namespaces, then a path to the network namespace.
	///
	/// Optional for DEL.
	pub netns: Option<PathBuf>,

	/// List of paths to search for CNI plugin executables.
	///
	/// This is in the same format as the host system’s `PATH` variable: e.g.
	/// separated by `:` on unix, and by `;` on Windows.
	pub path: Vec<PathBuf>,

	/// The input network configuration.
	pub config: NetworkConfig,
}

impl Cni {
	/// Converts this enum into an alternate representation which holds the Command separately from the inputs.
	///
	/// This is useful to deduplicate prep work between command implementations.
	pub fn into_inputs(self) -> Option<Inputs> {
		let command = match &self {
			Cni::Add { .. } => Command::Add,
			Cni::Del { .. } => Command::Del,
			Cni::Check { .. } => Command::Check,
			Cni::Version(_) => return None,
		};

		match self {
			Cni::Add {
				container_id,
				ifname,
				netns,
				path,
				config,
			}
			| Cni::Check {
				container_id,
				ifname,
				netns,
				path,
				config,
			} => Some(Inputs {
				command,
				container_id,
				ifname,
				netns: Some(netns),
				path,
				config,
			}),
			Cni::Del {
				container_id,
				ifname,
				netns,
				path,
				config,
			} => Some(Inputs {
				command,
				container_id,
				ifname,
				netns,
				path,
				config,
			}),
			Cni::Version(_) => unreachable!(),
		}
	}
}
