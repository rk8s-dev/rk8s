use std::str::FromStr;

use crate::error::InvalidCommandError;

/// Identifies the command given to a plugin.
///
/// For more information about the command semantics, see the spec or the
/// [`Cni`][crate::Cni] enum documentation.
#[derive(Clone, Copy, Debug)]
pub enum Command {
	/// The ADD command.
	Add,

	/// The DEL command.
	Del,

	/// The CHECK command.
	///
	/// Introduced in spec version 1.0.0.
	Check,

	/// The VERSION command.
	Version,
}

impl FromStr for Command {
	type Err = InvalidCommandError;

	/// Parses the Command from exactly ADD, DEL, CHECK, or VERSION only.
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"ADD" => Ok(Self::Add),
			"DEL" => Ok(Self::Del),
			"CHECK" => Ok(Self::Check),
			"VERSION" => Ok(Self::Version),
			_ => Err(InvalidCommandError),
		}
	}
}

impl AsRef<str> for Command {
	/// Returns one of ADD, DEL, CHECK, or VERSION.
	fn as_ref(&self) -> &'static str {
		match self {
			Command::Add => "ADD",
			Command::Del => "DEL",
			Command::Check => "CHECK",
			Command::Version => "VERSION",
		}
	}
}
