use crate::rt;
use crate::sandbox::sdk::SandboxClient;
use crate::sandbox::types::{SandboxCreateOptions, SandboxExecResult, SandboxExecSpec};
use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum SandboxCommand {
    /// Create a new sandbox handle
    Create(CreateArgs),
    /// Execute a command or Python code in a sandbox
    Exec(ExecArgs),
    /// Show sandbox metadata
    Inspect(InspectArgs),
    /// List all known sandboxes
    List,
    /// Stop a sandbox
    Stop(StopArgs),
    /// Remove a sandbox
    Rm(RmArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, default_value = "python:3.12-slim")]
    pub image: String,
    #[arg(long, default_value_t = 1)]
    pub cpus: u32,
    #[arg(long, default_value_t = 256)]
    pub memory_mib: u32,
    #[arg(long, default_value_t = false)]
    pub persistent: bool,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExecArgs {
    #[arg(value_name = "SANDBOX_ID")]
    pub sandbox_id: String,
    #[arg(long)]
    pub code: Option<String>,
    #[arg(long, default_value_t = 30)]
    pub timeout_secs: u64,
    #[arg(value_name = "COMMAND", trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    #[arg(value_name = "SANDBOX_ID")]
    pub sandbox_id: String,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    #[arg(value_name = "SANDBOX_ID")]
    pub sandbox_id: String,
}

#[derive(Args, Debug)]
pub struct RmArgs {
    #[arg(value_name = "SANDBOX_ID")]
    pub sandbox_id: String,
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

pub fn execute(cmd: SandboxCommand) -> Result<()> {
    let client = SandboxClient::new()?;

    match cmd {
        // Currently the create action only did the "Create" job, which will not start up sandbox
        // in this stage sandbox's status will always be `creating` until User use 'Exec' command
        SandboxCommand::Create(args) => {
            let sandbox = client.create(SandboxCreateOptions {
                image: args.image,
                cpus: args.cpus,
                memory_mib: args.memory_mib,
                persistent: args.persistent,
                name: args.name,
            })?;
            let info = sandbox.inspect()?;
            println!("sandbox_id={}", sandbox.id());
            println!("state={}", info.info.state);
            println!("store={}", client.root().display());
            Ok(())
        }
        SandboxCommand::Exec(args) => {
            let sandbox = client
                .get(args.sandbox_id)?
                .ok_or_else(|| anyhow!("sandbox not found"))?;
            let result = rt::block_on(async {
                // Here we provide a quick start interface `--code` to let user run a python command directly
                if let Some(code) = args.code {
                    sandbox
                        .execute(SandboxExecSpec::python(code).timeout_secs(args.timeout_secs))
                        .await
                } else {
                    let (command, cmd_args) = split_command(args.command)?;
                    sandbox
                        .execute(
                            SandboxExecSpec::command(command, cmd_args)
                                .timeout_secs(args.timeout_secs),
                        )
                        .await
                }
            })??;
            print_exec_result(&result);
            Ok(())
        }
        SandboxCommand::Inspect(args) => {
            let info = client.inspect(&args.sandbox_id)?;
            println!("{}", serde_json::to_string_pretty(&info.info)?);
            Ok(())
        }
        SandboxCommand::List => {
            for info in client.list()? {
                println!(
                    "{}\t{}\t{}",
                    info.info.id, info.info.state, info.info.spec.image
                );
            }
            Ok(())
        }
        SandboxCommand::Stop(args) => {
            let sandbox = client
                .get(args.sandbox_id)?
                .ok_or_else(|| anyhow!("sandbox not found"))?;
            rt::block_on(async { sandbox.stop().await })??;
            println!("sandbox stopped: {}", sandbox.id());
            Ok(())
        }
        SandboxCommand::Rm(args) => {
            rt::block_on(async { client.remove(&args.sandbox_id, args.force).await })??;
            println!("sandbox removed: {}", args.sandbox_id);
            Ok(())
        }
    }
}

fn split_command(command: Vec<String>) -> Result<(String, Vec<String>)> {
    let mut iter = command.into_iter();
    let cmd = iter
        .next()
        .ok_or_else(|| anyhow!("either --code or a command is required"))?;
    Ok((cmd, iter.collect()))
}

fn print_exec_result(result: &SandboxExecResult) {
    if !result.stdout.is_empty() {
        print!("{}", result.stdout);
    }
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }
    println!("exit_code={}", result.exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: SandboxCommand,
    }

    #[test]
    fn parse_create_command() {
        let cli = TestCli::parse_from([
            "rkforge",
            "create",
            "--image",
            "python:3.12-slim",
            "--cpus",
            "2",
            "--memory-mib",
            "512",
            "--persistent",
        ]);
        match cli.command {
            SandboxCommand::Create(args) => {
                assert_eq!(args.image, "python:3.12-slim");
                assert_eq!(args.cpus, 2);
                assert_eq!(args.memory_mib, 512);
                assert!(args.persistent);
            }
            _ => panic!("expected create command"),
        }
    }

    #[test]
    fn parse_exec_python_command() {
        let cli = TestCli::parse_from(["rkforge", "exec", "sbx-demo", "--code", "print('hello')"]);
        match cli.command {
            SandboxCommand::Exec(args) => {
                assert_eq!(args.sandbox_id, "sbx-demo");
                assert_eq!(args.code.as_deref(), Some("print('hello')"));
                assert!(args.command.is_empty());
            }
            _ => panic!("expected exec command"),
        }
    }
}
