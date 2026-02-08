//! Task definition of type Yaml.
//!
//! # The task type corresponding to the configuration file: [`YamlTask`]
//!
//! [`YamlTask`] implements the [`Task`] trait, which represents the tasks in the yaml
//! configuration file, and a yaml configuration file will be parsed into a series of [`YamlTask`].
//! It is different from `DefaultTask`, in addition to the four mandatory attributes of the
//! task type, he has several additional attributes.

use dagrs::{auto_node, Action, InChannels, NodeId, NodeTable, OutChannels};

/// Task struct for yaml file.
#[auto_node]
pub struct YamlTask {
    /// `yid` is the unique identifier defined in yaml, and `id` is the id assigned by the global id assigner.
    yid: String,
    /// Precursor identifier defined in yaml.
    precursors: Vec<String>,
    precursors_id: Vec<NodeId>,
}

impl YamlTask {
    #[allow(unused)]
    pub fn new(
        yaml_id: &str,
        precursors: Vec<String>,
        name: String,
        action: Box<dyn Action>,
        node_table: &mut NodeTable,
    ) -> Self {
        Self {
            yid: yaml_id.to_owned(),
            id: node_table.alloc_id_for(&name),
            name,
            precursors,
            precursors_id: Vec::new(),
            action,
            input_channels: InChannels::default(),
            output_channels: OutChannels::default(),
        }
    }
    /// After the configuration file is parsed, the id of each task has been assigned.
    /// At this time, the `precursors_id` of this task will be initialized according to
    /// the id of the predecessor task of each task.
    #[allow(unused)]
    pub fn init_precursors(&mut self, pres_id: Vec<NodeId>) {
        self.precursors_id = pres_id;
    }

    /// Get the precursor identifier defined in yaml.
    #[allow(unused)]
    pub fn str_precursors(&self) -> Vec<String> {
        self.precursors.clone()
    }
    /// Get the unique ID of the task defined in yaml.
    #[allow(unused)]
    pub fn str_id(&self) -> &str {
        &self.yid
    }
}
