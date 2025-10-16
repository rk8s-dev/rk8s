use anyhow::{Result, bail};
use dockerfile_parser::{
    ArgInstruction, BreakableStringComponent, CmdInstruction, CopyInstruction,
    EntrypointInstruction, EnvInstruction, FromInstruction, Instruction, LabelInstruction,
    RunInstruction, ShellOrExecExpr,
};
use std::path::{Path, PathBuf};

use crate::{
    image::context::StageContext as Context,
    pull::pull_or_get_image,
    storage::full_image_ref,
    task::{CopyTask, RunTask, TaskExec},
};

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
            _ => {
                bail!("Instruction {:?} is not supported", self);
            }
        }
    }
}

impl<P: AsRef<Path>> InstructionExt<P> for FromInstruction {
    fn execute(&self, ctx: &mut Context<P>) -> Result<()> {
        let (_from_flags, image_parsed) = (&self.flags, &self.image_parsed);

        let img_ref = full_image_ref(&image_parsed.image, image_parsed.tag.as_deref());

        let (_, layers) = pull_or_get_image(&img_ref, None::<String>)?;

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
                command_args.extend(vec!["/bin/sh".to_owned(), "-c".to_owned()]);
                for component in shell_expr.components.iter() {
                    match component {
                        BreakableStringComponent::Comment(_) => {}
                        BreakableStringComponent::String(spanned_string) => {
                            command_args.push(spanned_string.content.as_str().to_owned());
                        }
                    }
                }
            }
        }

        let envp: Vec<String> = ctx
            .image_config
            .envp
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();

        let task = RunTask {
            commands: command_args,
            envp,
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
            ctx.mount_config
                .mountpoint
                .join(dest.trim_start_matches('/'))
        } else {
            ctx.mount_config.mountpoint.join("root").join(dest)
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
