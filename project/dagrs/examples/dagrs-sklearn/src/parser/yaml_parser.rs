//! Default yaml configuration file parser.

use super::{FileContentError, YamlTask, YamlTaskError};
use crate::{
    command_action::CommandAction,
    parser::ParseError,
    utils::{file::load_file, parser::Parser},
};
use dagrs::{Action, EnvVar, Graph, Node, NodeId, NodeTable};
use std::collections::HashMap;
use yaml_rust::{Yaml, YamlLoader};

/// An implementation of [`Parser`]. It is the default yaml configuration file parser.
pub struct YamlParser;

impl YamlParser {
    /// Parses an item in the configuration file into a task.
    /// An item refers to:
    ///
    /// ```yaml
    ///    name: "Task 1"
    ///    after: [b, c]
    ///    cmd: echo a
    /// ```
    fn parse_one(
        &self,
        id: &str,
        item: &Yaml,
        specific_action: Option<Box<dyn Action>>,
        node_table: &mut NodeTable,
    ) -> Result<YamlTask, YamlTaskError> {
        // Get name first
        let name = item["name"]
            .as_str()
            .ok_or(YamlTaskError::NoNameAttr(id.to_owned()))?
            .to_owned();
        // precursors can be empty
        let mut precursors = Vec::new();
        if let Some(after_tasks) = item["after"].as_vec() {
            after_tasks
                .iter()
                .for_each(|task_id| precursors.push(task_id.as_str().unwrap().to_owned()));
        }

        if let Some(action) = specific_action {
            Ok(YamlTask::new(id, precursors, name, action, node_table))
        } else {
            let cmd_args = item["cmd"]
                .as_str()
                .ok_or(YamlTaskError::NoScriptAttr(name.clone()))?
                .split(' ')
                .collect::<Vec<_>>();

            let cmd = cmd_args.get(0).unwrap_or(&"");
            let args = cmd_args[1..].iter().map(|s| s.to_string()).collect();

            Ok(YamlTask::new(
                id,
                precursors,
                name,
                Box::new(CommandAction::new(cmd, args)),
                node_table,
            ))
        }
    }
}

impl Parser for YamlParser {
    fn parse_tasks(
        &self,
        file: &str,
        specific_actions: HashMap<String, Box<dyn Action>>,
    ) -> Result<(Graph, EnvVar), ParseError> {
        let content = load_file(file).map_err(|e| ParseError(e.to_string()))?;
        self.parse_tasks_from_str(&content, specific_actions)
    }

    fn parse_tasks_from_str(
        &self,
        content: &str,
        mut specific_actions: HashMap<String, Box<dyn Action>>,
    ) -> Result<(Graph, EnvVar), ParseError> {
        let mut node_table = NodeTable::default();
        // Parse Yaml
        let yaml_tasks =
            YamlLoader::load_from_str(content).map_err(FileContentError::IllegalYamlContent)?;
        if yaml_tasks.is_empty() {
            return Err(ParseError("No Tasks found".to_string()));
        }
        let yaml_tasks = yaml_tasks[0]["dagrs"]
            .as_hash()
            .ok_or(YamlTaskError::StartWordError)?;

        let mut tasks = Vec::with_capacity(yaml_tasks.len());
        let mut map = HashMap::with_capacity(yaml_tasks.len());
        // Read tasks
        for (v, w) in yaml_tasks {
            let id = v
                .as_str()
                .ok_or(ParseError("Invalid YAML Node Type".to_string()))?;
            let task = self.parse_one(id, w, specific_actions.remove(id), &mut node_table)?;
            map.insert(id, task.id());
            tasks.push(task);
        }

        let mut dag = Graph::new();
        let mut edges: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for task in tasks.iter_mut() {
            let mut pres = Vec::new();
            for pre in task.str_precursors() {
                if map.contains_key(&pre[..]) {
                    pres.push(map[&pre[..]]);
                } else {
                    return Err(YamlTaskError::NotFoundPrecursor(task.name().to_string()).into());
                }
            }

            let succ_id = task.id();
            pres.iter().for_each(|p| {
                if let Some(p) = edges.get_mut(&p) {
                    p.push(succ_id);
                } else {
                    edges.insert(*p, vec![succ_id]);
                }
            });

            task.init_precursors(pres.clone());
        }

        tasks.into_iter().for_each(|task| dag.add_node(task));
        edges.into_iter().for_each(|(x, ys)| {
            dag.add_edge(x, ys);
        });

        let env_var = EnvVar::new(node_table);
        dag.set_env(env_var.clone());

        Ok((dag, env_var))
    }
}
