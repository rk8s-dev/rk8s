use std::{collections::HashMap, path::Path};

use crate::{image::config::ImageConfig, overlayfs::MountConfig};

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
    pub global_args: &'ctx HashMap<String, Option<String>>,
    pub build_context: P,
}

impl<'ctx, P: AsRef<Path>> StageContext<'ctx, P> {
    pub fn new(
        mount_config: &'ctx mut MountConfig,
        image_config: &'ctx mut ImageConfig,
        image_aliases: &'ctx mut HashMap<String, String>,
        args: HashMap<String, Option<String>>,
        global_args: &'ctx HashMap<String, Option<String>>,
        build_context: P,
    ) -> Self {
        Self {
            mount_config,
            image_config,
            image_aliases,
            args,
            global_args,
            build_context,
        }
    }
}
