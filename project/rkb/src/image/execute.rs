use anyhow::{Result, bail};
use dockerfile_parser::{
    ArgInstruction, BreakableString, BreakableStringComponent, CmdInstruction, CopyInstruction,
    EntrypointInstruction, EnvInstruction, FromInstruction, Instruction, LabelInstruction,
    RunInstruction, ShellOrExecExpr,
};
use serde_json;
use std::path::{Path, PathBuf};

use crate::{
    image::{config::normalize_path, context::StageContext as Context},
    pull::sync_pull_or_get_image,
    storage::full_image_ref,
    task::{CopyTask, RunTask, TaskExec},
};

/// Extract the argument string from a BreakableString (used for Misc instructions like WORKDIR, USER).
fn extract_misc_argument(args: &BreakableString) -> String {
    let mut result = String::new();
    for component in args.components.iter() {
        match component {
            BreakableStringComponent::Comment(_) => {}
            BreakableStringComponent::String(spanned_string) => {
                result.push_str(&spanned_string.content);
            }
        }
    }
    result.trim().to_string()
}

/// An extension trait to execute dockerfile instructions.
pub trait InstructionExt<P: AsRef<Path>> {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()>;
}

impl<P: AsRef<Path>> InstructionExt<P> for Instruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        match self {
            Instruction::From(inst) => inst.execute(ctx),
            Instruction::Arg(inst) => inst.execute(ctx),
            Instruction::Label(inst) => inst.execute(ctx),
            Instruction::Run(inst) => inst.execute(ctx),
            Instruction::Entrypoint(inst) => inst.execute(ctx),
            Instruction::Cmd(inst) => inst.execute(ctx),
            Instruction::Copy(inst) => inst.execute(ctx),
            Instruction::Env(inst) => inst.execute(ctx),
            // Handle miscellaneous instructions
            Instruction::Misc(misc) => {
                let instr_name = misc.instruction.content.to_uppercase();
                match instr_name.as_str() {
                    "WORKDIR" => {
                        let workdir = extract_misc_argument(&misc.arguments);
                        if workdir.is_empty() {
                            bail!("WORKDIR requires a path argument");
                        }
                        tracing::debug!("Setting WORKDIR to: {}", workdir);
                        ctx.image_config.set_working_dir(workdir);
                        Ok(())
                    }
                    "USER" => {
                        let user = extract_misc_argument(&misc.arguments);
                        if user.is_empty() {
                            bail!("USER requires a user argument");
                        }
                        tracing::debug!("Setting USER to: {}", user);
                        ctx.image_config.set_user(user);
                        Ok(())
                    }
                    "VOLUME" => {
                        // Extract volume paths from arguments
                        // VOLUME supports multiple formats:
                        // - VOLUME /data
                        // - VOLUME /data /logs
                        // - VOLUME ["/data", "/logs"]
                        let volume_arg = extract_misc_argument(&misc.arguments);
                        if volume_arg.is_empty() {
                            bail!("VOLUME requires at least one path argument");
                        }

                        let volumes: Vec<String> = if volume_arg.starts_with('[') {
                            // JSON-array form: must be valid JSON, otherwise fail the build
                            match serde_json::from_str::<Vec<String>>(&volume_arg) {
                                Ok(v) => v,
                                Err(err) => {
                                    bail!("Invalid JSON array syntax in VOLUME instruction: {err}");
                                }
                            }
                        } else {
                            volume_arg
                                .split_whitespace()
                                .map(|s| s.to_string())
                                .collect()
                        };

                        for vol in volumes {
                            let vol = vol.trim().to_string();
                            if !vol.is_empty() {
                                tracing::debug!("Adding VOLUME: {}", vol);
                                ctx.image_config.add_volume(vol);
                            }
                        }
                        Ok(())
                    }
                    "STOPSIGNAL" => {
                        let signal = extract_misc_argument(&misc.arguments);
                        if signal.is_empty() {
                            bail!("STOPSIGNAL requires a signal argument");
                        }
                        tracing::debug!("Setting STOPSIGNAL to: {}", signal);
                        ctx.image_config.set_stop_signal(signal);
                        Ok(())
                    }
                    "EXPOSE" => {
                        // Extract exposed ports from arguments
                        // EXPOSE supports multiple formats:
                        // - EXPOSE 80
                        // - EXPOSE 80/tcp
                        // - EXPOSE 80/udp
                        // - EXPOSE 80 443 (multiple ports)
                        // - EXPOSE 80/tcp 443/tcp
                        let expose_arg = extract_misc_argument(&misc.arguments);
                        if expose_arg.is_empty() {
                            bail!("EXPOSE requires at least one port argument");
                        }

                        for port in expose_arg.split_whitespace() {
                            let port = port.trim().to_string();
                            if !port.is_empty() {
                                tracing::debug!("Adding EXPOSE: {}", port);
                                ctx.image_config.add_exposed_port(port);
                            }
                        }
                        Ok(())
                    }
                    "SHELL" => {
                        let shell_arg = extract_misc_argument(&misc.arguments);
                        if shell_arg.is_empty() {
                            bail!("SHELL requires a JSON array argument");
                        }

                        let shell: Vec<String> = match serde_json::from_str(&shell_arg) {
                            Ok(v) => v,
                            Err(e) => {
                                bail!("SHELL requires a valid JSON array: {}", e);
                            }
                        };

                        if shell.is_empty() {
                            bail!("SHELL array cannot be empty");
                        }

                        tracing::debug!("Setting SHELL to: {:?}", shell);
                        ctx.image_config.set_shell(shell);
                        Ok(())
                    }
                    // TODO: These instructions are currently ignored but should be properly
                    // recorded in the image config for OCI compliance
                    "HEALTHCHECK" | "ONBUILD" => {
                        tracing::warn!(
                            "Instruction {} is ignored (not yet implemented)",
                            instr_name
                        );
                        Ok(())
                    }
                    _ => {
                        bail!("Instruction {:?} is not supported", self);
                    }
                }
            }
        }
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for FromInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        let (_from_flags, image_parsed) = (&self.flags, &self.image_parsed);

        let img_ref = full_image_ref(&image_parsed.image, image_parsed.tag.as_deref());

        let (_, layers) = sync_pull_or_get_image(&img_ref, None::<String>)?;

        // add image alias mapping
        if let Some(alias) = &self.alias {
            ctx.image_aliases
                .insert(alias.content.clone(), img_ref.clone());
        }

        for layer in layers.iter() {
            ctx.mount_config.lower_dir.push(layer.clone());
        }

        // mount config should be unintialized
        ctx.mount_config.init()
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for ArgInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        let val = self.value.as_ref().map(|val| val.content.clone());
        ctx.args.insert(self.name.content.clone(), val);
        Ok(())
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for LabelInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        for label in self.labels.iter() {
            ctx.image_config
                .add_label(label.name.content.clone(), label.value.content.clone());
        }
        Ok(())
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for RunInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        let mut command_args = vec![];
        match &self.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                command_args = exec_expr
                    .as_str_vec()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            }
            ShellOrExecExpr::Shell(shell_expr) => {
                // Use custom shell if set, otherwise default to ["/bin/sh", "-c"]
                let shell = ctx.image_config.get_shell();
                command_args.extend(shell);
                let mut script = String::new();
                for component in shell_expr.components.iter() {
                    match component {
                        BreakableStringComponent::Comment(_) => {}
                        BreakableStringComponent::String(spanned_string) => {
                            script.push_str(&spanned_string.content);
                        }
                    }
                }
                command_args.push(script);
            }
        }
        // println!("Executing RUN command: {:?}", command_args);

        let envp: Vec<String> = ctx
            .image_config
            .envp
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();

        let task = RunTask {
            commands: command_args,
            envp,
            working_dir: ctx.image_config.working_dir.clone(),
            user: ctx.image_config.user.clone(),
        };
        task.execute(ctx.mount_config)
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for EntrypointInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        match &self.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                ctx.image_config.entrypoint = Some(
                    exec_expr
                        .as_str_vec()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                );
            }
            ShellOrExecExpr::Shell(shell_expr) => {
                let mut entrypoint = vec![];
                for component in shell_expr.components.iter() {
                    match component {
                        BreakableStringComponent::Comment(spanned_comment) => {
                            entrypoint.push(spanned_comment.content.clone());
                        }
                        BreakableStringComponent::String(spanned_string) => {
                            entrypoint.push(spanned_string.content.clone());
                        }
                    }
                }
                ctx.image_config.entrypoint = Some(entrypoint);
            }
        }
        Ok(())
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for CmdInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        match &self.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                ctx.image_config.cmd = Some(
                    exec_expr
                        .as_str_vec()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                );
            }
            ShellOrExecExpr::Shell(shell_expr) => {
                let mut cmd = vec![];
                for component in shell_expr.components.iter() {
                    match component {
                        BreakableStringComponent::Comment(spanned_comment) => {
                            cmd.push(spanned_comment.content.clone());
                        }
                        BreakableStringComponent::String(spanned_string) => {
                            cmd.push(spanned_string.content.clone());
                        }
                    }
                }
                ctx.image_config.cmd = Some(cmd);
            }
        }
        Ok(())
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for CopyInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        let flags = &self.flags;
        // TODO: Add flags support
        if !flags.is_empty() {
            bail!("Flags are not supported in COPY instruction");
        }

        let dest = self.destination.content.clone();
        let dest = if dest.starts_with('/') {
            // Absolute path - normalize to resolve `.` and `..` components
            let normalized = normalize_path(&dest);
            ctx.mount_config
                .mountpoint
                .join(normalized.trim_start_matches('/'))
        } else {
            // Relative path - resolve based on working directory, then normalize
            let working_dir = ctx.image_config.get_working_dir();
            let abs_dest = if working_dir == "/" {
                format!("/{}", dest)
            } else {
                format!("{}/{}", working_dir, dest)
            };
            // Normalize to resolve `..` segments and prevent path traversal
            let normalized = normalize_path(&abs_dest);
            ctx.mount_config
                .mountpoint
                .join(normalized.trim_start_matches('/'))
        };

        let build_ctx = ctx.build_context.as_ref().canonicalize()?;
        let src: Vec<PathBuf> = self
            .sources
            .iter()
            .map(|s| build_ctx.join(&s.content))
            .collect();

        let task = CopyTask { src, dest };
        task.execute(ctx.mount_config)
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for EnvInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        for var in self.vars.iter() {
            let mut val = Vec::new();
            for component in var.value.components.iter() {
                match component {
                    BreakableStringComponent::Comment(spanned_comment) => {
                        val.push(spanned_comment.content.clone());
                    }
                    BreakableStringComponent::String(spanned_string) => {
                        val.push(spanned_string.content.clone());
                    }
                }
            }
            ctx.image_config
                .add_envp(var.key.content.clone(), val.join(" "));
        }
        Ok(())
    }
}
