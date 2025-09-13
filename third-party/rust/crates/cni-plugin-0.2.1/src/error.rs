//! When CNI goes bad.

use std::env::VarError;

use regex::Regex;
use semver::Version;
use serde_json::Value;
use thiserror::Error;

use crate::reply::ErrorReply;

/// All errors emitted by this library, plus a few others.
#[derive(Debug, Error)]
pub enum CniError {
	/// Catch-all wrapper for I/O errors.
	#[error(transparent)]
	Io(#[from] std::io::Error),

	/// Catch-all wrapper for JSON serialization and deserialization.
	#[error(transparent)]
	Json(#[from] serde_json::Error),

	/// When the CNI version requested by the runtime is not supported.
	///
	/// The [`Version`] in the error is the CNI version provided, not ours.
	///
	/// Also see [`VersionReply`][crate::reply::VersionReply].
	#[error("plugin does not understand CNI version: {0}")]
	Incompatible(Version),

	/// When nothing is provided on STDIN.
	#[error("missing input network config")]
	MissingInput,

	/// When a delegated plugin doesn’t output anything on STDOUT.
	#[error("missing plugin output")]
	MissingOutput,

	/// When a required environment variable is missing.
	#[error("missing environment variable: {var}: {err}")]
	MissingEnv {
		/// the variable name
		var: &'static str,

		/// the underlying error
		#[source]
		err: VarError,
	},

	/// When an environment variable couldn’t be parsed or is invalid.
	#[error("environment variable has invalid format: {var}: {err}")]
	InvalidEnv {
		/// the variable name
		var: &'static str,

		/// the underlying error
		#[source]
		err: Box<dyn std::error::Error>,
	},

	/// When the current working directory cannot be obtained (for delegation).
	#[error("cannot obtain current working directory")]
	NoCwd,

	/// When a delegated plugin cannot be found on `CNI_PATH`.
	#[error("missing (or not on CNI_PATH) plugin {name}: {err}")]
	MissingPlugin {
		/// the name of the plugin binary
		name: String,

		/// the underlying error
		#[source]
		err: which::Error,
	},

	/// Wrapper for errors in relation to a delegated plugin.
	#[error("with plugin {plugin}: {err}")]
	Delegated {
		/// the name of the plugin binary
		plugin: String,

		/// the underlying error
		err: Box<Self>,
	},

	/// A generic error as a string.
	///
	/// This error variant is not used in the library, but is provided for
	/// plugin implementations to make use of without needing to make their own
	/// error type.
	///
	/// # Example
	///
	/// ```
	/// # use cni_plugin::error::CniError;
	/// CniError::Generic("a total catastrophe".into());
	/// ```
	#[error("{0}")]
	Generic(String),

	/// A debug error as anything that implements [`Debug`][std::fmt::Debug].
	///
	/// This error variant is not used in the library, but is provided for
	/// plugin implementations to make use of without needing to make their own
	/// error type.
	///
	/// # Example
	///
	/// ```
	/// # use cni_plugin::error::CniError;
	/// CniError::Debug(Box::new(("hello", "world", vec![1, 2, 3])));
	/// ```
	#[error("{0:?}")]
	Debug(Box<dyn std::fmt::Debug>),

	/// When a field in configuration is missing.
	///
	/// This error variant is not used in the library, but is provided for
	/// plugin implementations to make use of without needing to make their own
	/// error type.
	///
	/// # Example
	///
	/// ```
	/// # use cni_plugin::error::CniError;
	/// CniError::MissingField("ipam.type");
	/// ```
	#[error("can't proceed without {0} field")]
	MissingField(&'static str),

	/// When a field in configuration is invalid.
	///
	/// This error variant is not used in the library, but is provided for
	/// plugin implementations to make use of without needing to make their own
	/// error type.
	///
	/// # Example
	///
	/// ```
	/// # use cni_plugin::error::CniError;
	/// # use serde_json::Value;
	/// CniError::InvalidField {
	///     field: "ipam.pool",
	///     expected: "string",
	///     value: Value::Null,
	/// };
	/// ```
	#[error("{field}: expected {expected}, got: {value:?}")]
	InvalidField {
		/// the name or path of the invalid field
		field: &'static str,

		/// the value or type the field was expected to be
		expected: &'static str,

		/// the actual value or a facsimile thereof
		value: Value,
	},
}

impl CniError {
	/// Convert a CniError into an ErrorReply.
	///
	/// [`ErrorReply`]s can be used with [`reply`][crate::reply::reply], but
	/// require `cni_version` to be set to the input configuration’s. This
	/// method makes it easier to create errors (including with the `?`
	/// operator, from foreign error types) and only populate the version field
	/// when ready to send the reply.
	///
	/// It’s recommended to add an implementation of this if you make your own
	/// error type.
	pub fn into_reply(self, cni_version: Version) -> ErrorReply<'static> {
		match self {
			Self::Io(e) => ErrorReply {
				cni_version,
				code: 5,
				msg: "I/O error",
				details: e.to_string(),
			},
			Self::Json(e) => ErrorReply {
				cni_version,
				code: 6,
				msg: "Cannot decode JSON payload",
				details: e.to_string(),
			},
			e @ Self::Incompatible(_) => ErrorReply {
				cni_version,
				code: 1,
				msg: "Incompatible CNI version",
				details: e.to_string(),
			},
			e @ Self::MissingInput => ErrorReply {
				cni_version,
				code: 7,
				msg: "Missing payload",
				details: e.to_string(),
			},
			e @ Self::MissingOutput => ErrorReply {
				cni_version,
				code: 7,
				msg: "Missing output",
				details: e.to_string(),
			},
			e @ Self::MissingEnv { .. } => ErrorReply {
				cni_version,
				code: 4,
				msg: "Missing environment variable",
				details: e.to_string(),
			},
			e @ Self::InvalidEnv { .. } => ErrorReply {
				cni_version,
				code: 4,
				msg: "Invalid environment variable",
				details: e.to_string(),
			},
			e @ Self::NoCwd => ErrorReply {
				cni_version,
				code: 5,
				msg: "Bad workdir",
				details: e.to_string(),
			},
			e @ Self::MissingPlugin { .. } => ErrorReply {
				cni_version,
				code: 5,
				msg: "Missing plugin",
				details: e.to_string(),
			},
			e @ Self::Delegated { .. } => ErrorReply {
				cni_version,
				code: 5,
				msg: "Delegated",
				details: e.to_string(),
			},
			Self::Generic(s) => ErrorReply {
				cni_version,
				code: 100,
				msg: "ERROR",
				details: s,
			},
			e @ Self::Debug { .. } => ErrorReply {
				cni_version,
				code: 101,
				msg: "DEBUG",
				details: e.to_string(),
			},
			e @ Self::MissingField(_) => ErrorReply {
				cni_version,
				code: 104,
				msg: "Missing field",
				details: e.to_string(),
			},
			e @ Self::InvalidField { .. } => ErrorReply {
				cni_version,
				code: 107,
				msg: "Invalid field",
				details: e.to_string(),
			},
		}
	}
}

/// Underlying error used for an empty value that shouldn’t be.
///
/// Used with [`CniError::InvalidEnv`].
#[derive(Clone, Copy, Debug, Error)]
#[error("must not be empty")]
pub struct EmptyValueError;

/// Underlying error used for an invalid `CNI_COMMAND`.
///
/// Used with [`CniError::InvalidEnv`].
#[derive(Clone, Copy, Debug, Error)]
#[error("must be one of ADD, DEL, CHECK, VERSION")]
pub struct InvalidCommandError;

/// Underlying error used for a value that should match a regex but doesn’t.
///
/// Used with [`CniError::InvalidEnv`].
#[derive(Clone, Debug, Error)]
#[error("must match regex: {0}")]
pub struct RegexValueError(pub Regex);
