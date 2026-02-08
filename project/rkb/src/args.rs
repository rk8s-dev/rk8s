use crate::commands::{ComposeCommand, PodCommand, VolumeCommand};
use crate::{copy, image, login, logout, overlayfs, pull, push, repo, run};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rkb", about = "A container runtime and management tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build a container image from Dockerfile
    Build(image::BuildArgs),
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
    /// Run a command in a new container
    Run(RunArgs),
    /// Start one or more containers
    Start(StartArgs),
    /// Display the status of a container
    State(StateArgs),
    /// Manage volumes
    #[command(subcommand)]
    Volume(VolumeCommand),
    #[command(hide = true)]
    ExecInternal(run::ExecInternalArgs),
}

/// Run a command in a new container
#[derive(Parser, Debug, Clone)]
pub struct RunArgs {
    #[arg(value_name = "CONTAINER_YAML")]
    pub container_yaml: String,
    #[arg(long, short = 'v')]
    pub volumes: Option<Vec<String>>,
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
