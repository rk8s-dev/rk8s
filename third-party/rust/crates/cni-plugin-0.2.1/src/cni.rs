use std::{
	env,
	io::{stdin, Read},
	path::PathBuf,
	str::FromStr,
};

use log::{debug, error};
use regex::Regex;
use semver::Version;

use crate::{
	command::Command,
	config::NetworkConfig,
	error::{CniError, EmptyValueError, RegexValueError},
	path::CniPath,
	reply::reply,
	version::VersionPayload,
};

/// The main entrypoint to this plugin and the enum which contains plugin input.
///
/// See the field definitions on [`Inputs`][crate::Inputs] for more details on
/// the command subfields.
#[derive(Clone, Debug)]
pub enum Cni {
	/// The ADD command: add namespace to network, or apply modifications.
	///
	/// A CNI plugin, upon receiving an ADD command, should either:
	/// - create the interface defined by `ifname` inside the namespace at the
	///   `netns` path, or
	/// - adjust the configuration of the interface defined by `ifname` inside
	///   the namespace at the `netns` path.
	///
	/// More details in the [spec](https://github.com/containernetworking/cni/blob/master/SPEC.md#add-add-container-to-network-or-apply-modifications).
	Add {
		/// The container ID, as provided by the runtime.
		container_id: String,

		/// The name of the interface inside the container.
		ifname: String,

		/// The container’s “isolation domain” or namespace path.
		netns: PathBuf,

		/// List of paths to search for CNI plugin executables.
		path: Vec<PathBuf>,

		/// The input network configuration.
		config: NetworkConfig,
	},

	/// The DEL command: remove namespace from network, or un-apply modifications.
	///
	/// A CNI plugin, upon receiving a DEL command, should either:
	/// - delete the interface defined by `ifname` inside the namespace at the
	///   `netns` path, or
	/// - undo any modifications applied in the plugin's ADD functionality.
	///
	/// More details in the [spec](https://github.com/containernetworking/cni/blob/master/SPEC.md#del-remove-container-from-network-or-un-apply-modifications).
	Del {
		/// The container ID, as provided by the runtime.
		container_id: String,

		/// The name of the interface inside the container.
		ifname: String,

		/// The container’s “isolation domain” or namespace path.
		///
		/// May not be provided for DEL commands.
		netns: Option<PathBuf>,

		/// List of paths to search for CNI plugin executables.
		path: Vec<PathBuf>,

		/// The input network configuration.
		config: NetworkConfig,
	},

	/// The CHECK command: check that a namespace's networking is as expected.
	///
	/// This was introduced in CNI spec v1.0.0.
	///
	/// More details in the [spec](https://github.com/containernetworking/cni/blob/master/SPEC.md#check-check-containers-networking-is-as-expected).
	Check {
		/// The container ID, as provided by the runtime.
		container_id: String,

		/// The name of the interface inside the container.
		ifname: String,

		/// The container’s “isolation domain” or namespace path.
		netns: PathBuf,

		/// List of paths to search for CNI plugin executables.
		path: Vec<PathBuf>,

		/// The input network configuration.
		config: NetworkConfig,
	},

	/// The VERSION command: used to probe plugin version support.
	///
	/// The plugin should reply with a [`VersionReply`][crate::reply::VersionReply].
	///
	/// Note that when using [`Cni::load()`], this command is already handled,
	/// and you should mark this [`unreachable!()`].
	Version(Version),
}

impl Cni {
	/// Reads the plugin inputs from the environment and STDIN.
	///
	/// This reads _and validates_ the required `CNI_*` environment variables,
	/// and the STDIN for a JSON-encoded input object, but it does not output
	/// anything to STDOUT nor exits the process, nor does it panic.
	///
	/// Note that [as per convention][args-deprecation], the `CNI_ARGS` variable
	/// is deprecated, and this library deliberately chooses to ignore it. You
	/// may of course read and parse it yourself.
	///
	/// A number of things are logged in here. If you have used
	/// [`install_logger`][crate::install_logger], this may result in output
	/// being sent to STDERR (and/or to file).
	///
	/// In general you should prefer [`Cni::load()`].
	///
	/// [args-deprecation]: https://github.com/containernetworking/cni/blob/master/CONVENTIONS.md#cni_args
	pub fn from_env() -> Result<Self, CniError> {
		fn require_env<T>(var: &'static str) -> Result<T, CniError>
		where
			T: FromStr,
			T::Err: std::error::Error + 'static,
		{
			env::var(var)
				.map_err(|err| CniError::MissingEnv { var, err })
				.and_then(|val| {
					debug!("read env var {} = {:?}", var, val);
					val.parse().map_err(|err| CniError::InvalidEnv {
						var,
						err: Box::new(err),
					})
				})
		}
		fn load_env<T>(var: &'static str) -> Result<Option<T>, CniError>
		where
			T: FromStr,
			T::Err: std::error::Error + 'static,
		{
			require_env(var).map(Some).or_else(|err| {
				if let CniError::MissingEnv { .. } = err {
					Ok(None)
				} else {
					Err(err)
				}
			})
		}

		let path: CniPath = load_env("CNI_PATH")?.unwrap_or_default();
		let path = path.0;

		let mut payload = Vec::with_capacity(1024);
		debug!("reading stdin til EOF...");
		stdin().read_to_end(&mut payload)?;
		debug!("read payload bytes={}", payload.len());
		if payload.is_empty() {
			return Err(CniError::MissingInput);
		}

		fn check_container_id(id: &str) -> Result<(), CniError> {
			if id.is_empty() {
				return Err(CniError::InvalidEnv {
					var: "CNI_CONTAINERID",
					err: Box::new(EmptyValueError),
				});
			}

			let re = Regex::new(r"^[a-z0-9][a-z0-9_.\-]*$").unwrap();
			if !re.is_match(id) {
				return Err(CniError::InvalidEnv {
					var: "CNI_CONTAINERID",
					err: Box::new(RegexValueError(re)),
				});
			}

			Ok(())
		}

		match require_env("CNI_COMMAND")? {
			Command::Add => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Add {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: require_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Del => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Del {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: load_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Check => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Check {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: require_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Version => {
				let config: VersionPayload = serde_json::from_slice(&payload)?;
				Ok(Self::Version(config.cni_version))
			}
		}
	}

	/// Reads the plugin inputs from the environment and STDIN and reacts to errors and the VERSION command.
	///
	/// This does the same thing as [`Cni::from_env()`] but it also immediately
	/// replies to the `VERSION` command, and also immediately replies if errors
	/// result from reading the inputs, both of which write to STDOUT and exit.
	///
	/// This version also logs a debug message with the name and version of this
	/// library crate.
	pub fn load() -> Self {
		debug!(
			"CNI plugin built with {} crate version {}",
			env!("CARGO_PKG_NAME"),
			env!("CARGO_PKG_VERSION")
		);

		let cni_version = Version::parse("1.0.0").unwrap();

		match Self::from_env() {
			Err(e) => {
				error!("{}", e);
				reply(e.into_reply(cni_version))
			}
			Ok(Cni::Version(v)) => Self::handle_version(v),
			Ok(c) => c,
		}
	}

	// TODO: parse network config (administrator) files
	// maybe also with something that searches in common locations
}
