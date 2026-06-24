use crate::commands::{ComposeCommand, PodCommand, VolumeCommand, config_cli::ConfigArgs};
#[cfg(feature = "sandbox")]
use crate::sandbox::agent::SandboxAgentArgs;
#[cfg(feature = "sandbox")]
use crate::sandbox::cli::SandboxCommand;
#[cfg(feature = "sandbox")]
use crate::sandbox::guest::SandboxGuestInitArgs;
#[cfg(feature = "sandbox")]
use crate::sandbox::vm::SandboxShimArgs;
use crate::{copy, image, images, login, logout, overlayfs, pull, push, repo, run};
use clap::{ArgAction, Parser, Subcommand};

const RUN_AFTER_HELP: &str = "\
Examples:
  rkforge run container.yaml --device /dev/dri/card0
  rkforge run container.yaml --device /dev/nvidia0:/dev/nvidia0:rw --device /dev/nvidiactl
";

#[derive(Parser, Debug)]
#[command(name = "rkforge", about = "A container runtime and management tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build a container image from Dockerfile
    Build(image::BuildArgs),
    /// Get or set rkforge configuration
    Config(ConfigArgs),
    /// Manage container compositions
    #[command(subcommand)]
    Compose(ComposeCommand),
    #[command(hide = true)]
    Cleanup(overlayfs::CleanupArgs),
    #[command(hide = true)]
    Copy(copy::CopyArgs),
    /// Create a new container
    Create(CreateArgs),
    /// Delete one or more containers
    Delete(DeleteArgs),
    /// Execute a command in a running container
    Exec(Box<ExecArgs>),
    /// List local images
    #[command(alias = "ls")]
    Images(images::ImagesArgs),
    /// Display detailed information about an image
    Inspect(images::InspectArgs),
    /// Load an image from a tar archive
    Load(images::LoadArgs),
    /// Login to distribution server
    Login(login::LoginArgs),
    /// Logout from distribution server
    Logout(logout::LogoutArgs),
    #[command(hide = true)]
    Mount(overlayfs::MountArgs),
    /// Pull an image from specific distribution server.
    Pull(pull::PullArgs),
    /// Push an image to specific distribution server.
    Push(push::PushArgs),
    /// Manage pods
    #[command(subcommand)]
    Pod(PodCommand),
    /// List containers
    Ps(PsArgs),
    /// List and manage repositories
    Repo(repo::RepoArgs),
    /// Remove one or more local images
    Rmi(images::RmiArgs),
    /// Run a command in a new container
    Run(RunArgs),
    /// Save an image to a tar archive
    Save(images::SaveArgs),
    /// Manage AI sandboxes
    #[cfg(feature = "sandbox")]
    #[command(subcommand)]
    Sandbox(SandboxCommand),
    #[cfg(feature = "sandbox")]
    #[command(hide = true, name = "sandbox-shim")]
    SandboxShim(SandboxShimArgs),
    #[cfg(feature = "sandbox")]
    #[command(hide = true, name = "sandbox-agent")]
    SandboxAgent(SandboxAgentArgs),
    #[cfg(feature = "sandbox")]
    #[command(hide = true, name = "sandbox-guest-init")]
    SandboxGuestInit(SandboxGuestInitArgs),
    /// Start one or more containers
    Start(StartArgs),
    /// Display the status of a container
    State(StateArgs),
    /// Create a tag for a local image
    Tag(images::TagArgs),
    /// Manage volumes
    #[command(subcommand)]
    Volume(VolumeCommand),
    #[command(hide = true)]
    ExecInternal(run::ExecInternalArgs),
    /// Kill a running container
    Kill(KillArgs),
    /// Stop a running container (SIGTERM + wait)
    Stop(StopArgs),
    /// Wait until a container exits
    Wait(WaitArgs),
    /// Remove one or more containers
    Rm(RmArgs),
    /// Attach to a running container's stdio
    Attach(AttachArgs),
}

/// Run a command in a new container
#[derive(Parser, Debug, Clone)]
#[command(after_help = RUN_AFTER_HELP)]
pub struct RunArgs {
    #[arg(value_name = "CONTAINER_YAML")]
    pub container_yaml: String,
    #[arg(long, short = 'v')]
    pub volumes: Option<Vec<String>>,
    /// Add a host device to the container.
    /// Format: HOST_PATH[:CONTAINER_PATH[:PERMISSIONS]]
    #[arg(
        long,
        action = ArgAction::Append,
        value_name = "HOST_PATH[:CONTAINER_PATH[:PERMISSIONS]]"
    )]
    pub device: Vec<String>,
}

/// Create a new container
#[derive(Parser, Debug, Clone)]
pub struct CreateArgs {
    #[arg(value_name = "CONTAINER_YAML")]
    pub container_yaml: String,
    #[arg(long, short = 'v')]
    pub volumes: Option<Vec<String>>,
}

/// Start one or more containers
#[derive(Parser, Debug, Clone)]
pub struct StartArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,
}

/// Delete one or more containers
#[derive(Parser, Debug, Clone)]
pub struct DeleteArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,
}

/// Display the status of a container
#[derive(Parser, Debug, Clone)]
pub struct StateArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,
}

/// List containers
#[derive(Parser, Debug, Clone)]
pub struct PsArgs {
    /// Only display container IDs default is false
    #[arg(long, short)]
    pub quiet: Option<bool>,
    /// Specify the format (default or table)
    #[arg(long, short)]
    pub format: Option<String>,
}

/// Kill a container
#[derive(Parser, Debug, Clone)]
pub struct KillArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,

    /// Signal to send (default: SIGKILL)
    #[arg(long, default_value = "KILL")]
    pub signal: String,
}

/// Stop a container (SIGTERM + wait)
#[derive(Parser, Debug, Clone)]
pub struct StopArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,

    /// Timeout in seconds before SIGKILL
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

/// Wait for a container to exit
#[derive(Parser, Debug, Clone)]
pub struct WaitArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,

    /// Timeout in seconds (0 means wait indefinitely).
    /// Note: exit code is always 0 if libcontainer does not expose the actual exit code.
    #[arg(long, default_value = "0")]
    pub timeout: u64,
}

/// Remove one or more containers
#[derive(Parser, Debug, Clone)]
pub struct RmArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: Option<String>,

    /// Force removal of a running container
    #[arg(long, short = 'f')]
    pub force: bool,

    /// Remove all stopped containers
    #[arg(long, short = 'a')]
    pub all: bool,
}

/// Attach to a running container
#[derive(Parser, Debug, Clone)]
pub struct AttachArgs {
    #[arg(value_name = "CONTAINER_NAME")]
    pub container_name: String,
}

/// Execute a command in a running container
#[derive(Parser, Debug, Clone)]
pub struct ExecArgs {
    /// Unix socket (file) path , which will receive file descriptor of the writing end of the pseudoterminal
    #[clap(long)]
    pub console_socket: Option<std::path::PathBuf>,
    #[clap(long)]
    /// Current working directory of the container
    pub cwd: Option<std::path::PathBuf>,
    /// Environment variables that should be set in the container
    #[clap(short, long, value_parser = parse_env::<String, String>, number_of_values = 1)]
    pub env: Vec<(String, String)>,
    #[clap(short, long)]
    pub tty: bool,
    /// Run the command as a user
    #[clap(short, long, value_parser = parse_user::<u32, u32>)]
    pub user: Option<(u32, Option<u32>)>,
    /// Add additional group IDs. Can be specified multiple times
    #[clap(long, short = 'g', number_of_values = 1)]
    pub additional_gids: Vec<u32>,
    /// Path to process.json
    #[clap(short, long)]
    pub process: Option<std::path::PathBuf>,
    /// Detach from the container process
    #[clap(short, long)]
    pub detach: bool,
    #[clap(long)]
    /// The file to which the pid of the container process should be written to
    pub pid_file: Option<std::path::PathBuf>,
    /// Set the asm process label for the process commonly used with selinux
    #[clap(long)]
    pub process_label: Option<String>,
    /// Set the apparmor profile for the process
    #[clap(long)]
    pub apparmor: Option<String>,
    /// Prevent the process from gaining additional privileges
    #[clap(long)]
    pub no_new_privs: bool,
    /// Add a capability to the bounding set for the process
    #[clap(long, number_of_values = 1)]
    pub cap: Vec<String>,
    /// Pass N additional file descriptors to the container
    #[clap(long, default_value = "0")]
    pub preserve_fds: i32,
    /// Allow exec in a paused container
    #[clap(long)]
    pub ignore_paused: bool,
    /// Execute a process in a sub-cgroup
    #[clap(long)]
    pub cgroup: Option<String>,
    #[arg(value_name = "CONTAINER_ID")]
    pub container_id: String,
    /// Command that should be executed in the container
    #[arg(required = false)]
    pub command: Vec<String>,
    #[arg(long, required = false)]
    pub root_path: Option<String>,
}

fn parse_env<T, U>(s: &str) -> Result<(T, U), Box<dyn std::error::Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: std::error::Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid VAR=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

fn parse_user<T, U>(
    s: &str,
) -> Result<(T, Option<U>), Box<dyn std::error::Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: std::error::Error + Send + Sync + 'static,
{
    if let Some(pos) = s.find(':') {
        Ok((s[..pos].parse()?, Some(s[pos + 1..].parse()?)))
    } else {
        Ok((s.parse()?, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn run_args_accept_multiple_devices() {
        let cli = Cli::parse_from([
            "rkforge",
            "run",
            "container.yaml",
            "--device",
            "/dev/nvidia0",
            "--device",
            "/dev/nvidiactl:/dev/nvidiactl:rw",
        ]);

        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.container_yaml, "container.yaml");
                assert_eq!(
                    args.device,
                    vec![
                        "/dev/nvidia0".to_string(),
                        "/dev/nvidiactl:/dev/nvidiactl:rw".to_string(),
                    ]
                );
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_help_mentions_device_flag_and_examples() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("run")
            .expect("run subcommand to exist")
            .render_long_help()
            .to_string();

        assert!(help.contains("--device"));
        assert!(help.contains("/dev/dri/card0"));
    }
}
