use std::{collections::HashMap, path::Path};

use crate::{
    image::{BuildProgressMode, config::ImageConfig},
    overlayfs::MountConfig,
};

/// Represents the execution context for a single build stage.
///
/// This struct acts as a state machine, holding all the necessary and mutable
/// information required during the image building process. Each instruction
/// (`FROM`, `RUN`, `COPY`, etc.) will modify this context, for example,
/// by adding layers to the mount configuration, setting environment variables
/// in the image configuration, or resolving build arguments.
pub struct StageContext<'ctx, P: AsRef<Path>> {
    pub mount_config: &'ctx mut MountConfig,
    pub image_config: &'ctx mut ImageConfig,
    pub image_aliases: &'ctx mut HashMap<String, String>,
    pub args: HashMap<String, Option<String>>,
    pub cli_build_args: &'ctx HashMap<String, String>,
    pub global_args: &'ctx HashMap<String, Option<String>>,
    pub build_context: P,
    pub no_cache: bool,
    pub quiet: bool,
    pub progress_mode: BuildProgressMode,
}
