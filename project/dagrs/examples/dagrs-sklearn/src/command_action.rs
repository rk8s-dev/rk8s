use std::process::Command;

use dagrs::async_trait::async_trait;
use dagrs::{Action, Content, Output};

/// [`CommandAction`] is a specific implementation of [`Complex`], used to execute operating system commands.
pub struct CommandAction {
    command: String,
    args: Vec<String>,
}

impl CommandAction {
    #[allow(unused)]
    pub fn new(cmd: &str, args: Vec<String>) -> Self {
        Self {
            command: cmd.to_owned(),
            args,
        }
    }
}

#[async_trait]
impl Action for CommandAction {
    async fn run(
        &self,
        in_channels: &mut dagrs::InChannels,
        out_channels: &mut dagrs::OutChannels,
        _: std::sync::Arc<dagrs::EnvVar>,
    ) -> dagrs::Output {
        let mut args = Vec::new();
        let mut cmd = if cfg!(target_os = "windows") {
            args.push("-Command");
            args.push(&self.command);
            Command::new("powershell")
        } else {
            Command::new(&self.command)
        };

        let mut inputs = vec![];
        in_channels
            .map(|input| {
                if let Ok(inp) = input {
                    if let Some(inp) = inp.get::<String>() {
                        inputs.push(inp.to_owned());
                    }
                }
            })
            .await;
        args.append(&mut self.args.iter().map(|s| s.as_str()).collect());
        args.append(&mut inputs.iter().map(|x| x.as_str()).collect());

        log::info!("cmd: {:?}, args: {:?}", cmd.get_program(), args);

        let out = match cmd.args(args).output() {
            Ok(o) => o,
            Err(e) => {
                out_channels.broadcast(Content::new(e.raw_os_error())).await;
                return Output::error_with_exit_code(
                    e.raw_os_error(),
                    Some(Content::new(e.to_string())),
                );
            }
        };
        let code = out.status.code().unwrap_or(0);
        let stdout: Vec<String> = {
            let out = String::from_utf8(out.stdout).unwrap_or("".to_string());
            if cfg!(target_os = "windows") {
                out.rsplit_terminator("\r\n").map(str::to_string).collect()
            } else {
                out.split_terminator('\n').map(str::to_string).collect()
            }
        };
        let stderr: Vec<String> = {
            let out = String::from_utf8(out.stderr).unwrap_or("".to_string());
            if cfg!(target_os = "windows") {
                out.rsplit_terminator("\r\n").map(str::to_string).collect()
            } else {
                out.split_terminator('\n').map(str::to_string).collect()
            }
        };
        if out.status.success() {
            out_channels
                .broadcast(Content::new((stdout.clone(), stderr.clone())))
                .await;
            Output::new((stdout.clone(), stderr.clone()))
        } else {
            out_channels
                .broadcast(Content::new((stdout.clone(), stderr.clone())))
                .await;
            let output = Content::new((stdout, stderr));
            Output::error_with_exit_code(Some(code), Some(output))
        }
    }
}
