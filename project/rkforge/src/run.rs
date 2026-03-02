use crate::image::build_runtime::{BuildHostEntry, BuildUlimit};
use crate::overlayfs::{
    bind_mount, do_exec, prepare_hosts, prepare_network, prepare_shm, switch_namespace,
};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use serde::de::DeserializeOwned;
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
    #[arg(long)]
    pub add_hosts_base64: Option<String>,
    #[arg(long)]
    pub ulimits_base64: Option<String>,
    #[arg(long)]
    pub shm_size: Option<u64>,
}

fn decode_optional_base64_json<T>(value: Option<&str>, arg_name: &str) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    let Some(value) = value else {
        return Ok(T::default());
    };

    let data = general_purpose::STANDARD
        .decode(value)
        .with_context(|| format!("Failed to decode {arg_name} from base64"))?;
    serde_json::from_slice(&data).with_context(|| format!("Failed to deserialize {arg_name}"))
}

pub fn exec_internal(args: ExecInternalArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    let mountpoint = Path::new(&args.mountpoint);
    let add_hosts: Vec<BuildHostEntry> =
        decode_optional_base64_json(args.add_hosts_base64.as_deref(), "add-hosts-base64")?;
    let ulimits: Vec<BuildUlimit> =
        decode_optional_base64_json(args.ulimits_base64.as_deref(), "ulimits-base64")?;

    bind_mount(mountpoint)?;
    prepare_network(mountpoint)?;
    prepare_hosts(mountpoint, &add_hosts)?;
    prepare_shm(mountpoint, args.shm_size)?;

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

    // Pass raw user string to do_exec; user resolution happens after chroot
    // so it reads the container's /etc/passwd instead of the host's.
    do_exec(
        mountpoint,
        &commands,
        &envp,
        working_dir,
        args.user.as_deref(),
        &ulimits,
    )?;
    unreachable!();
}
