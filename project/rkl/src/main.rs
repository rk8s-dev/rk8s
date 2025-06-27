use clap::{Parser, Subcommand};
use rkl::cli_commands;

#[derive(Parser)]
#[command(name = "rkl")]
#[command(
    about = "A simple container runtime", 
    long_about = None,  
    override_usage = "rkl <workload> <command> [OPTIONS]",
)]
struct Cli {
    #[command(subcommand)]
    workload: Workload,
}

impl Cli {
    pub fn run(self) -> Result<(), anyhow::Error> {
            match self.workload {
            Workload::Pod(cmd) => match cmd {
                PodCommand::Run { pod_yaml } => cli_commands::run_pod(&pod_yaml),
                PodCommand::Create { pod_yaml } => cli_commands::create_pod(&pod_yaml),
                PodCommand::Start { pod_name } => cli_commands::start_pod(&pod_name),
                PodCommand::Delete { pod_name } => cli_commands::delete_pod(&pod_name),
                PodCommand::State { pod_name } => cli_commands::state_pod(&pod_name),
                PodCommand::Exec(exec) => {
                    let exit_code = cli_commands::exec_pod(*exec)?;
                    std::process::exit(exit_code);
                }
            },
            Workload::Container(cmd) => match cmd {
                ContainerCommand::Run { container_yaml,  } => cli_commands::run_container(&container_yaml),
                ContainerCommand::Start { container_name,  } => cli_commands::start_container(&container_name),
                ContainerCommand::State { container_name,  } => cli_commands::state_container(&container_name),
                ContainerCommand::Delete { container_name,  } => cli_commands::delete_container(&container_name),
                ContainerCommand::Create { container_yaml,  } => cli_commands::create_container(&container_yaml),
                ContainerCommand::Exec(exec) => {
                    let exit_code = cli_commands::exec_container(*exec)?;
                    std::process::exit(exit_code)
                }
            },
            Workload::Compose(cmd) => match cmd {
                ComposeCommand::Up { compose_yaml } => cli_commands::run_compose(compose_yaml),
                ComposeCommand::Down { } => cli_commands::run_compose(compose_yaml),
            },
}
    }
    }

/// define the 3 state for the run command "container" "pod" "compose"
#[derive(Subcommand)]
enum Workload {
    #[command(subcommand, about = "Operations related to pods")]
    Pod(PodCommand),

    #[command(subcommand, about = "Manage standalone containers")]
    Container(ContainerCommand),

    #[command(subcommand, about = "Manage multi-container apps using Compose")]
    Compose(ComposeCommand),
}

#[derive(Subcommand)]
enum ContainerCommand {
    #[command(about = "Run a single container from a YAML file using rkl run container.yaml")]
    Run {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,
    },
    #[command(about = "Create a Container from a YAML file using rkl create container.yaml")]
    Create {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,
    },
    #[command(about = "Start a Container with a Container-name using rkl start container-name")]
    Start {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },

    #[command(about = "Delete a Container with a Container-name using rkl delete container-name")]
    Delete {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },
    #[command(about = "Get the state of a Container using rkl state Container-name")]
    State {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },
    Exec(Box<rkl::commands::exec_cli::ExecContainer>),
}

#[derive(Subcommand)]
enum ComposeCommand {


    #[command(about = "Start a from a compose yaml")]
    Up {
        #[arg(value_name = "COMPOSE_YAML")]
        compose_yaml: Option<String>,
        
    },
    Down {
        // #[arg(value_name = "COMPOSE_YAML")]
        // compose_yaml: String,
    },
}

#[derive(Subcommand)]
enum PodCommand {
    #[command(about = "Run a pod from a YAML file using rkl run pod.yaml")]
    Run {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    #[command(about = "Create a pod from a YAML file using rkl create pod.yaml")]
    Create {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    #[command(about = "Start a pod with a pod-name using rkl start pod-name")]
    Start {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Delete a pod with a pod-name using rkl delete pod-name")]
    Delete {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },
    #[command(about = "Get the state of a pod using rkl state pod-name")]
    State {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },
    Exec(Box<rkl::commands::exec_cli::ExecPod>),
}

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    cli.run()
}
