use std::collections::HashMap;

use crate::{
    connection::information_packet::Content,
    node::node::{NodeTable, NODE_TABLE_STR},
};

pub type Variable = Content;

/// # Environment variable.
///
/// When multiple nodes are running, they may need to share the same data or read
/// the same configuration information. Environment variables can meet this requirement.
/// Before all nodes run, the user builds a [`EnvVar`] and sets all the environment
/// variables. One [`EnvVar`] corresponds to one dag. All nodes in a job can
/// be shared and immutable at runtime. environment variables.
///
/// Variables that [`EnvVar`] should have:
/// - [NodeTable] : a mapping from node's name to `NodeId`.
/// During the runtime of a `Graph`, [`NodeTable`] allows
/// each `Node` to look up the id of a specific node by its name.
#[derive(Debug, Clone)]
pub struct EnvVar {
    variables: HashMap<String, Variable>,
}

impl EnvVar {
    /// Allocate a new [`EnvVar`].
    pub fn new(node_table: NodeTable) -> Self {
        let mut env = Self {
            variables: HashMap::default(),
        };
        env.set(NODE_TABLE_STR, node_table);
        env
    }

    #[allow(unused)]
    /// Set a global variables.
    ///
    /// # Example
    /// ```rust
    /// use dagrs::{EnvVar, NodeTable};
    ///
    /// # let mut env = EnvVar::new(NodeTable::default());
    /// env.set("Hello", "World".to_string());
    /// ```
    pub fn set<H: Send + Sync + 'static>(&mut self, name: &str, var: H) {
        let mut v = Variable::new(var);
        self.variables.insert(name.to_owned(), v);
    }

    /// Get environment variables through keys of type &str.
    ///
    /// Note: This method will clone the value. To avoid cloning, use `get_ref`.
    pub fn get<H: Send + Sync + Clone + 'static>(&self, name: &str) -> Option<H> {
        self.get_ref(name).cloned()
    }

    /// Get environment variables through keys of type &str.
    pub fn get_ref<H: Send + Sync + 'static>(&self, name: &str) -> Option<&H> {
        if let Some(content) = self.variables.get(name) {
            content.get()
        } else {
            None
        }
    }
}
