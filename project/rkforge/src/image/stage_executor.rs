use std::path::Path;

use anyhow::{Context, Result};
use dockerfile_parser::{Instruction, Stage};

use crate::image::BuildProgressMode;
use crate::image::context::StageContext;
use crate::image::execute::InstructionExt;

fn instruction_name(inst: &Instruction) -> String {
    match inst {
        Instruction::From(_) => "FROM".to_string(),
        Instruction::Arg(_) => "ARG".to_string(),
        Instruction::Label(_) => "LABEL".to_string(),
        Instruction::Run(_) => "RUN".to_string(),
        Instruction::Entrypoint(_) => "ENTRYPOINT".to_string(),
        Instruction::Cmd(_) => "CMD".to_string(),
        Instruction::Copy(_) => "COPY".to_string(),
        Instruction::Env(_) => "ENV".to_string(),
        Instruction::Misc(misc) => misc.instruction.content.to_ascii_uppercase(),
    }
}

/// A stage executor responsible for running all instructions within a single build `Stage`.
///
/// It holds a reference to the `Stage` it needs to execute and a `StageContext`, which
/// provides mutable access to the shared state from the main `Executor` (like mount
/// configurations and image metadata). Its primary role is to iterate through the
/// instructions of its assigned stage and delegate the execution to each instruction via
/// the `InstructionExt` trait.
pub struct StageExecutor<'a, P: AsRef<Path>> {
    ctx: StageContext<'a, P>,
    stage: Stage<'a>,
}

impl<'a, P: AsRef<Path>> StageExecutor<'a, P> {
    pub fn new(ctx: StageContext<'a, P>, stage: Stage<'a>) -> Self {
        Self { ctx, stage }
    }

    pub fn execute(&mut self) -> Result<()> {
        self.stage
            .instructions
            .iter()
            .enumerate()
            .try_for_each(|(idx, inst)| {
                if !self.ctx.quiet && self.ctx.progress_mode == BuildProgressMode::Plain {
                    println!("  -> {inst:?}");
                } else if !self.ctx.quiet && self.ctx.progress_mode == BuildProgressMode::Tty {
                    let total = self.stage.instructions.len();
                    println!("  -> [{}/{}] {}", idx + 1, total, instruction_name(inst));
                }
                inst.execute(&mut self.ctx)
                    .with_context(|| format!("Failed to execute instruction: {inst:?}"))
            })
            .context("Failed to execute stage")
    }
}
