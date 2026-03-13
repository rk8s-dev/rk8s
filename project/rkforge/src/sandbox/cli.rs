use crate::rt;
use crate::sandbox::{ExecRequest, SandboxOptions, SandboxRuntime};
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
    let runtime = SandboxRuntime::new()?;

    match cmd {
        SandboxCommand::Create(args) => {
            let sandbox = runtime.create(SandboxOptions {
                image: args.image,
                cpus: args.cpus,
                memory_mib: args.memory_mib,
                persistent: args.persistent,
                name: args.name,
            })?;
            let info = sandbox.info()?;
            println!("sandbox_id={}", sandbox.id);
            println!("state={}", info.state);
            println!("store={}", runtime.root().display());
            Ok(())
        }
        SandboxCommand::Exec(args) => {
            let sandbox = runtime
                .get(args.sandbox_id)?
                .ok_or_else(|| anyhow!("sandbox not found"))?;
            let result = rt::block_on(async {
                if let Some(code) = args.code {
                    let mut request = ExecRequest::new(sandbox.id.clone(), "python3");
                    request.inline_code = Some(code);
                    request.language = Some("python".to_string());
                    request.timeout_secs = Some(args.timeout_secs);
                    sandbox.exec_request(request).await
                } else {
                    let (command, cmd_args) = split_command(args.command)?;
                    let mut request = ExecRequest::new(sandbox.id.clone(), command);
                    request.args = cmd_args;
                    request.timeout_secs = Some(args.timeout_secs);
                    sandbox.exec_request(request).await
                }
            })??;
            print_exec_result(&result);
            Ok(())
        }
        SandboxCommand::Inspect(args) => {
            let info = runtime
                .get_info(&args.sandbox_id)?
                .ok_or_else(|| anyhow!("sandbox not found"))?;
            println!("{}", serde_json::to_string_pretty(&info)?);
            Ok(())
        }
        SandboxCommand::List => {
            for info in runtime.list()? {
                println!("{}\t{}\t{}", info.id, info.state, info.spec.image);
            }
            Ok(())
        }
        SandboxCommand::Stop(args) => {
            let sandbox = runtime
                .get(args.sandbox_id)?
                .ok_or_else(|| anyhow!("sandbox not found"))?;
            rt::block_on(async { sandbox.stop().await })??;
            println!("sandbox stopped: {}", sandbox.id);
            Ok(())
        }
        SandboxCommand::Rm(args) => {
            rt::block_on(async { runtime.remove(&args.sandbox_id, args.force).await })??;
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

fn print_exec_result(result: &crate::sandbox::ExecResult) {
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
