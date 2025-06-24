use clap::{Parser, Subcommand};
use rkl::cli_commands;

#[derive(Parser)]
#[command(name = "rkl")]
#[command(about = "A simple container runtime", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn run(self) -> Result<(), anyhow::Error> {
        match self.command {
            Commands::Run { run_type } => run_type.run(),
            Commands::Create { pod_yaml } => cli_commands::create_pod(&pod_yaml),
            Commands::Start { pod_name } => cli_commands::start_pod(&pod_name),
            Commands::Delete { pod_name } => cli_commands::delete_pod(&pod_name),
            Commands::State { pod_name } => cli_commands::state_pod(&pod_name),
            Commands::Exec(exec) => {
                let exit_code = cli_commands::exec_pod(*exec)?;
                std::process::exit(exit_code);
            }
        }
    }
}

/// define the 3 state for the run command "container" "pod" "compose"
#[derive(Subcommand, Clone)]
enum RunType {
    /// Run a pod from a YAML file using ./rkl run pod.yaml
    #[clap(name = "pod")]
    Pod {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    /// Run a single container directly
    #[clap(name = "container")]
    Container {
        #[arg(value_name = "CAONTAINER_YAML")]
        container_yaml: String,
    },

    /// Run docker compose YAML
    #[clap(name = "compose")]
    Compose {
        #[arg(value_name = "COMPOSE_YAML")]
        compose_yaml: String,
    },
}

impl RunType {
    pub fn run(&self) -> Result<(), anyhow::Error> {
        match self {
            RunType::Pod { pod_yaml } => cli_commands::run_pod(&pod_yaml),
            RunType::Container { container_yaml } => cli_commands::run_container(&container_yaml),
            RunType::Compose { compose_yaml } => cli_commands::run_compose(&compose_yaml),
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Run a pod, a single container, or a compose YAML file")]
    Run {
        #[command(subcommand)]
        run_type: RunType,
    },
    #[command(about = "Create a pod from a YAML file using ./rkl create pod.yaml")]
    Create {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    #[command(about = "Start a pod with a pod-name using ./rkl start pod-name")]
    Start {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Delete a pod with a pod-name using ./rkl delete pod-name")]
    Delete {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },
    #[command(about = "Get the state of a pod using ./rkl state pod-name")]
    State {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },
    Exec(Box<rkl::commands::exec_cli::Exec>),
}

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    cli.run()
}
