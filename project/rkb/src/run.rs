use crate::overlayfs::{UserSpec, bind_mount, do_exec, prepare_network, switch_namespace};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use std::{ffi::CString, path::Path};

#[derive(Debug, Parser, Clone)]
pub struct ExecInternalArgs {
    #[arg(long)]
    pub mountpoint: String,
    #[arg(long)]
    pub envp_base64: String,
    /// In order to support commands with arguments(e.g. `-c`),
    /// we use base64 to encode the entire command list as a single argument.
    #[arg(long)]
    pub commands_base64: String,
    /// Working directory for command execution. If not specified, defaults to "/".
    #[arg(long)]
    pub working_dir: Option<String>,
    /// User to run the command as. Format: "user", "uid", "user:group", "uid:gid", etc.
    #[arg(long)]
    pub user: Option<String>,
}

pub fn exec_internal(args: ExecInternalArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    let mountpoint = Path::new(&args.mountpoint);
    prepare_network(mountpoint)?;
    bind_mount(mountpoint)?;

    let commands_json = general_purpose::STANDARD
        .decode(&args.commands_base64)
        .context("Failed to decode commands from base64")?;
    let commands: Vec<String> = serde_json::from_slice(&commands_json)
        .context("Failed to deserialize commands from json")?;
    let commands = commands.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    if commands.is_empty() {
        bail!("At least one command is required to run");
    }

    let envp_json = general_purpose::STANDARD
        .decode(&args.envp_base64)
        .context("Failed to decode envp from base64")?;
    let envp: Vec<String> =
        serde_json::from_slice(&envp_json).context("Failed to deserialize envp from json")?;
    let envp = envp
        .iter()
        .map(|s| CString::new(s.as_bytes()).context("Environment variable contains null byte"))
        .collect::<Result<Vec<_>>>()?;

    // Get working directory, default to "/"
    let working_dir = args.working_dir.as_deref().unwrap_or("/");

    // Parse user specification if provided
    let user_spec = args.user.as_ref().map(|u| parse_user_spec(u)).transpose()?;

    do_exec(mountpoint, &commands, &envp, working_dir, user_spec)?;
    unreachable!();
}

/// Parse a user specification string into uid and optional gid.
/// Supported formats: "user", "uid", "user:group", "uid:gid", "uid:group", "user:gid"
fn parse_user_spec(user_str: &str) -> Result<UserSpec> {
    let parts: Vec<&str> = user_str.split(':').collect();

    let uid = parse_uid_or_username(parts[0])?;
    let gid = if parts.len() > 1 {
        Some(parse_gid_or_groupname(parts[1])?)
    } else {
        None
    };

    Ok(UserSpec { uid, gid })
}

/// Parse a string as either a numeric uid or a username.
fn parse_uid_or_username(s: &str) -> Result<u32> {
    // Try parsing as numeric uid first
    if let Ok(uid) = s.parse::<u32>() {
        return Ok(uid);
    }

    // Try looking up as username using uzers crate
    use uzers::get_user_by_name;
    if let Some(user) = get_user_by_name(s) {
        return Ok(user.uid());
    }

    bail!("Unknown user: {}", s)
}

/// Parse a string as either a numeric gid or a group name.
fn parse_gid_or_groupname(s: &str) -> Result<u32> {
    // Try parsing as numeric gid first
    if let Ok(gid) = s.parse::<u32>() {
        return Ok(gid);
    }

    // Try looking up as group name using uzers crate
    use uzers::get_group_by_name;
    if let Some(group) = get_group_by_name(s) {
        return Ok(group.gid());
    }

    bail!("Unknown group: {}", s)
}
