//! Plugin delegation.
//!
//! This module is only available when either of the **with-smol** (for smol and
//! async-std async runtimes) or **with-tokio** features are enabled. If both
//! are provided, the crate won't compile.

use std::{
	env,
	io::Cursor,
	path::Path,
	process::{ExitStatus, Stdio},
};

use log::{debug, error, info};
use which::which_in;

use crate::{config::NetworkConfig, error::CniError, reply::ReplyPayload, Command};

/// Run a plugin as delegate.
///
/// You will want to refer to [Section 4 of the spec][spec-§4] extensively for
/// the semantics to apply when doing plugin delegation.
///
/// The return type is to be the expected success reply type, so either
/// [`SuccessReply`][crate::reply::SuccessReply] or
/// [`IpamSuccessReply`][crate::reply::IpamSuccessReply].
///
/// STDIN and STDOUT are handled as per spec, and STDERR is hooked up directly
/// to the calling process's STDERR.
///
/// # Errors
///
/// This method errors if:
/// - the current directory can’t be obtained
/// - the `CNI_PATH` variable is missing
/// - the `CNI_PATH` doesn't contain the `sub_plugin` (as per `PATH` logic)
/// - the delegate plugin errors
///
/// [spec-§4]: https://github.com/containernetworking/cni/blob/master/SPEC.md#section-4-plugin-delegation
pub async fn delegate<S>(
	sub_plugin: &str,
	command: Command,
	config: &NetworkConfig,
) -> Result<S, CniError>
where
	S: for<'de> ReplyPayload<'de>,
{
	let cwd = env::current_dir().map_err(|_| CniError::NoCwd)?;
	let plugin = which_in(
		sub_plugin,
		Some(env::var("CNI_PATH").map_err(|err| CniError::MissingEnv {
			var: "CNI_PATH",
			err,
		})?),
		cwd,
	)
	.map_err(|err| CniError::MissingPlugin {
		name: sub_plugin.into(),
		err,
	})?;

	let config_bytes = serde_json::to_vec(config).map_err(|err| CniError::Delegated {
		plugin: sub_plugin.into(),
		err: Box::new(err.into()),
	})?;

	match delegate_command(&plugin, command, &config_bytes).await {
		Ok((status, stdout)) => {
			if stdout.is_empty() {
				if matches!(command, Command::Add) {
					delegate_command(&plugin, Command::Del, &config_bytes)
						.await
						.map_err(|err| CniError::Delegated {
							plugin: sub_plugin.into(),
							err: Box::new(err),
						})?;
				}

				return Err(CniError::Delegated {
					plugin: sub_plugin.into(),
					err: Box::new(CniError::MissingOutput),
				});
			}

			if status.success() {
				let reader = Cursor::new(stdout);
				Ok(
					serde_json::from_reader(reader).map_err(|err| CniError::Delegated {
						plugin: sub_plugin.into(),
						err: Box::new(err.into()),
					})?,
				)
			} else {
				if matches!(command, Command::Add) {
					delegate_command(&plugin, Command::Del, &config_bytes)
						.await
						.map_err(|err| CniError::Delegated {
							plugin: sub_plugin.into(),
							err: Box::new(err),
						})?;
				}

				Err(CniError::Delegated {
					plugin: sub_plugin.into(),
					err: Box::new(CniError::Generic(String::from_utf8_lossy(&stdout).into())),
				})
			}
		}
		Err(err) => {
			error!("error running delegate: {}", err);
			if matches!(command, Command::Add) {
				// We're already failing pretty badly so this is a Just In Case, but
				// in all likelihood won't work either. So we ignore any failure.
				delegate_command(&plugin, Command::Del, &config_bytes)
					.await
					.ok();
			}

			Err(CniError::Delegated {
				plugin: sub_plugin.into(),
				err: Box::new(err),
			})
		}
	}
}

#[cfg(feature = "with-smol")]
async fn delegate_command(
	plugin: impl AsRef<Path>,
	command: impl AsRef<str>,
	stdin_bytes: &[u8],
) -> Result<(ExitStatus, Vec<u8>), CniError> {
	use async_process::Command;
	use futures::io::{copy, AsyncWriteExt, Cursor};

	let plugin = plugin.as_ref();
	let command = command.as_ref();

	info!(
		"delegating to plugin at {} for command={}",
		plugin.display(),
		command
	);

	debug!("spawing child process, async=smol");
	let mut child = Command::new(plugin)
		.env("CNI_COMMAND", command)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::inherit())
		.spawn()?;

	{
		debug!("taking child stdin");
		let mut stdin = child.stdin.take().unwrap();
		// UNWRAP: stdin configured above

		debug!("copying bytes={} to stdin", stdin_bytes.len());
		let bytes = Cursor::new(stdin_bytes);
		let written = copy(bytes, &mut stdin).await?;

		debug!("closing stdin");
		stdin.close().await?;

		assert_eq!(written as usize, stdin_bytes.len());
		debug!("dropping stdin handle");
	}

	debug!("awaiting child");
	let output = child.output().await?;

	info!(
		"delegate plugin at {} for command={} has returned with {}; stdout bytes={}",
		plugin.display(),
		command,
		output.status,
		output.stdout.len()
	);
	Ok((output.status, output.stdout))
}

#[cfg(feature = "with-tokio")]
async fn delegate_command(
	plugin: impl AsRef<Path>,
	command: impl AsRef<str>,
	mut stdin_bytes: &[u8],
) -> Result<(ExitStatus, Vec<u8>), CniError> {
	use tokio::io::copy_buf;
	use tokio::process::Command;

	let plugin = plugin.as_ref();
	let command = command.as_ref();

	info!(
		"delegating to plugin at {} for command={}",
		plugin.display(),
		command
	);

	debug!("spawing child process, async=tokio");
	let mut child = Command::new(plugin)
		.env("CNI_COMMAND", command)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::inherit())
		.spawn()?;

	{
		debug!("taking child stdin");
		let mut stdin = child.stdin.take().unwrap();
		// UNWRAP: stdin configured above

		debug!("copying bytes={} to stdin", stdin_bytes.len());
		let written = copy_buf(&mut stdin_bytes, &mut stdin).await?;
		assert_eq!(written as usize, stdin_bytes.len());

		debug!("dropping stdin handle");
	}

	debug!("awaiting child");
	let output = child.wait_with_output().await?;

	info!(
		"delegate plugin at {} for command={} has returned with {}; stdout bytes={}",
		plugin.display(),
		command,
		output.status,
		output.stdout.len()
	);
	Ok((output.status, output.stdout))
}
