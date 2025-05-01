use std::fs::File;

use clap::{Parser, Subcommand};
use daemonize::Daemonize;
use rkl::{cli_commands, daemon};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "rkl")]
#[command(about = "A simple container runtime", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Run a pod from a YAML file using ./rkl run pod.yaml")]
    Run {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
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
    // Run as a daemon process.
    // For convenient, I won't remove cli part now.
    Daemon,
}

fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { pod_yaml } => cli_commands::run_pod(&pod_yaml),
        Commands::Create { pod_yaml } => cli_commands::create_pod(&pod_yaml),
        Commands::Start { pod_name } => cli_commands::start_pod(&pod_name),
        Commands::Delete { pod_name } => cli_commands::delete_pod(&pod_name),
        Commands::State { pod_name } => cli_commands::state_pod(&pod_name),
        Commands::Exec(exec) => {
            let exit_code = cli_commands::exec_pod(*exec)?;
            std::process::exit(exit_code);
        }
        Commands::Daemon => {
            let time_stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let out = File::create(format!("/etc/rk8s/log_{}.out", time_stamp)).unwrap();
            let err = File::create(format!("/etc/rk8s/log_{}.err", time_stamp)).unwrap();
            let pid = format!("/tmp/rkl_{}.pid", time_stamp);
            let daemonize = Daemonize::new().pid_file(&pid).stdout(out).stderr(err);
            daemonize.start()?;
            daemon::main()
        }
    }
}
