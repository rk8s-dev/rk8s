use super::{config::ImageConfig, config::StageExecutorConfig};
use crate::exec::Task;
use crate::{overlayfs::MountConfig, registry::pull_or_get_image, run::exec_task};
use anyhow::{Context, Result, bail};
use dockerfile_parser::{
    ArgInstruction, BreakableStringComponent, CmdInstruction, CopyInstruction,
    EntrypointInstruction, EnvInstruction, FromInstruction, Instruction, LabelInstruction,
    RunInstruction, ShellOrExecExpr, Stage,
};
use std::{collections::HashMap, env, path::PathBuf};

/// StageExecutor bundles up what we need to know when executing one stage of a
/// (possibly multi-stage) build.
///
/// Each stage may need to produce an image to be used as the base in a later
/// stage (with the last stage's image being the end product of the build), and
/// it may need to leave its working container in place so that the container's
/// root filesystem's contents can be used as the source for a COPY instruction
/// in a later stage.
///
/// Each stage has its own base image, so it starts with its own configuration
/// and set of volumes.
///
/// If we're naming the result of the build, only the last stage will apply that
/// name to the image that it produces.
///
/// [Reference](https://github.com/containers/buildah/blob/main/imagebuildah/stage_executor.go)
pub struct StageExecutor<'m> {
    pub mount_config: &'m mut MountConfig,
    pub image_config: &'m mut ImageConfig,
    /// Map image aliases to their paths
    pub image_aliases: &'m mut HashMap<String, String>,
    /// Args created in the stage
    pub args: HashMap<String, Option<String>>,
}

impl<'m> StageExecutor<'m> {
    pub fn new(
        mount_config: &'m mut MountConfig,
        image_config: &'m mut ImageConfig,
        image_aliases: &'m mut HashMap<String, String>,
    ) -> Self {
        Self {
            mount_config,
            image_config,
            image_aliases,
            args: HashMap::new(),
        }
    }

    pub fn execute(&mut self, stage: &Stage<'_>, config: &StageExecutorConfig) -> Result<()> {
        for instruction in stage.instructions.iter() {
            self.execute_instruction(instruction, config)
                .with_context(|| format!("Failed to execute instruction {instruction:?}"))?;
        }
        Ok(())
    }

    /// TODO: When parsing values, check if they are in the args map
    fn execute_instruction(
        &mut self,
        instruction: &Instruction,
        config: &StageExecutorConfig,
    ) -> Result<()> {
        tracing::info!("Executing instruction: {instruction:?}");
        match instruction {
            Instruction::From(from_instruction) => {
                self.execute_from_instruction(from_instruction)?;
            }
            Instruction::Arg(arg_instruction) => {
                self.execute_arg_instruction(arg_instruction)?;
            }
            Instruction::Label(label_instruction) => {
                self.execute_label_instruction(label_instruction)?;
            }
            Instruction::Run(run_instruction) => {
                self.execute_run_instruction(run_instruction, config)?;
            }
            Instruction::Entrypoint(entrypoint_instruction) => {
                self.execute_entrypoint_instruction(entrypoint_instruction)?;
            }
            Instruction::Cmd(cmd_instruction) => {
                self.execute_cmd_instruction(cmd_instruction)?;
            }
            Instruction::Copy(copy_instruction) => {
                self.execute_copy_instruction(copy_instruction, config)?;
            }
            Instruction::Env(env_instruction) => {
                self.execute_env_instruction(env_instruction)?;
            }
            Instruction::Misc(misc_instruction) => {
                bail!("Unsupported instruction: {misc_instruction:?}");
            }
        }
        Ok(())
    }

    fn execute_from_instruction(&mut self, from_instruction: &FromInstruction) -> Result<()> {
        let _from_flags = &from_instruction.flags;
        let image_parsed = &from_instruction.image_parsed;

        // TODO: Handle from_flags
        let img_ref = format!("{image_parsed}");
        let rt = tokio::runtime::Runtime::new()?;

        let image = rt.block_on(async { pull_or_get_image(&img_ref).await })?;

        // add image alias mapping
        if let Some(alias) = &from_instruction.alias {
            self.image_aliases
                .insert(alias.content.clone(), img_ref.clone());
        }

        for layer in image.layers.iter() {
            self.mount_config.lower_dir.push(layer.clone());
        }

        // mount config should be unintialized
        self.mount_config.init()?;

        Ok(())
    }

    fn execute_arg_instruction(&mut self, arg_instruction: &ArgInstruction) -> Result<()> {
        let val = arg_instruction
            .value
            .as_ref()
            .map(|val| val.content.clone());
        self.args.insert(arg_instruction.name.content.clone(), val);
        Ok(())
    }

    fn execute_label_instruction(&mut self, label_instruction: &LabelInstruction) -> Result<()> {
        for label in label_instruction.labels.iter() {
            self.image_config
                .add_label(label.name.content.clone(), label.value.content.clone());
        }
        Ok(())
    }

    fn execute_run_instruction(
        &mut self,
        run_instruction: &RunInstruction,
        _config: &StageExecutorConfig,
    ) -> Result<()> {
        let mut commands = vec![];
        match &run_instruction.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                commands = exec_expr
                    .as_str_vec()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            }
            ShellOrExecExpr::Shell(shell_expr) => {
                commands.extend(vec!["/bin/bash".to_owned(), "-c".to_owned()]);
                for component in shell_expr.components.iter() {
                    match component {
                        BreakableStringComponent::Comment(_) => {}
                        BreakableStringComponent::String(spanned_string) => {
                            commands.push(spanned_string.content.as_str().to_owned());
                        }
                    }
                }
            }
        }

        let envp: Vec<String> = self
            .image_config
            .envp
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();

        self.mount_config.prepare()?;

        let mount_config = self.mount_config.clone();
        exec_task(
            mount_config,
            Task::Run {
                command: commands,
                envp,
            },
        )?;

        self.mount_config.finish()?;
        Ok(())
    }

    fn execute_entrypoint_instruction(
        &mut self,
        entrypoint_instruction: &EntrypointInstruction,
    ) -> Result<()> {
        match &entrypoint_instruction.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                self.image_config.entrypoint = Some(
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
                self.image_config.entrypoint = Some(entrypoint);
            }
        }
        Ok(())
    }

    fn execute_cmd_instruction(&mut self, cmd_instruction: &CmdInstruction) -> Result<()> {
        match &cmd_instruction.expr {
            ShellOrExecExpr::Exec(exec_expr) => {
                self.image_config.cmd = Some(
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
                self.image_config.cmd = Some(cmd);
            }
        }
        Ok(())
    }

    fn execute_copy_instruction(
        &mut self,
        copy_instruction: &CopyInstruction,
        _config: &StageExecutorConfig,
    ) -> Result<()> {
        let flags = &copy_instruction.flags;
        // TODO: Add flags support
        if !flags.is_empty() {
            bail!("Flags are not supported in COPY instruction");
        }

        self.mount_config.prepare()?;

        let dest = copy_instruction.destination.content.clone();
        let dest = if dest.starts_with('/') {
            self.mount_config
                .mountpoint
                .join(dest.trim_start_matches('/'))
        } else {
            self.mount_config.mountpoint.join("root").join(dest)
        };

        let current_dir = env::current_dir()?;
        let src: Vec<PathBuf> = copy_instruction
            .sources
            .iter()
            .map(|s| current_dir.join(&s.content))
            .collect();

        let mount_config = self.mount_config.clone();
        exec_task(
            mount_config,
            Task::Copy {
                src: src
                    .into_iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect(),
                dest: dest.to_string_lossy().into_owned(),
            },
        )?;

        self.mount_config.finish()?;
        Ok(())
    }

    fn execute_env_instruction(&mut self, env_instruction: &EnvInstruction) -> Result<()> {
        for var in env_instruction.vars.iter() {
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
            self.image_config
                .add_envp(var.key.content.clone(), val.join(" "));
        }
        Ok(())
    }
}
