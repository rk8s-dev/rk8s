use std::path::Path;

use anyhow::{Context, Result};
use dockerfile_parser::Stage;

use crate::image::context::StageContext;
use crate::image::execute::InstructionExt;

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
            .try_for_each(|inst| {
                inst.execute(&mut self.ctx)
                    .with_context(|| format!("Failed to execute instruction: {inst:?}"))
            })
            .context("Failed to execute stage")
    }
}
