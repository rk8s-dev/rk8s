use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    connection::{in_channel::InChannels, out_channel::OutChannels},
    utils::{env::EnvVar, output::Output},
};

use super::{
    action::{Action, EmptyAction},
    node::{Node, NodeId, NodeName, NodeTable},
};

/// # Default node type
///
/// [`DefaultNode`] is a default implementation of the [`Node`] trait. Users can use this node
/// type to build tasks to meet most needs.
///
/// ## Create a `DefaultNode`:
/// - use the method `new`. Required attributes: node's name; [`NodeTable`](for id allocation).
///
/// ```rust
/// use dagrs::{NodeName, NodeTable, DefaultNode};
///
/// let node_name = "Node X";
/// let mut node_table = NodeTable::new();
/// let mut node = DefaultNode::new(
///     NodeName::from(node_name),
///     &mut node_table,
/// );
/// ```
///
/// - use the method `with_action`. Required attributes: node's name; [`NodeTable`](for id allocation);
/// execution logic [`Action`].
///
/// ```rust
/// use dagrs::{NodeName, NodeTable, DefaultNode, EmptyAction};
///
/// let node_name = "Node X";
/// let mut node_table = NodeTable::new();
/// let mut node = DefaultNode::with_action(
///     NodeName::from(node_name),
///     EmptyAction,
///     &mut node_table,
/// );
/// ```
pub struct DefaultNode {
    id: NodeId,
    name: NodeName,
    action: Box<dyn Action>,
    in_channels: InChannels,
    out_channels: OutChannels,
}
#[async_trait]
impl Node for DefaultNode {
    fn id(&self) -> NodeId {
        self.id
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }

    fn input_channels(&mut self) -> &mut InChannels {
        &mut self.in_channels
    }

    fn output_channels(&mut self) -> &mut OutChannels {
        &mut self.out_channels
    }

    async fn run(&mut self, env: Arc<EnvVar>) -> Output {
        self.action
            .run(&mut self.in_channels, &mut self.out_channels, env)
            .await
    }
}

impl DefaultNode {
    pub fn new(name: NodeName, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            action: Box::new(EmptyAction),
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
        }
    }

    pub fn with_action(
        name: NodeName,
        action: impl Action + 'static,
        node_table: &mut NodeTable,
    ) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            action: Box::new(action),
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
        }
    }

    pub fn set_action(&mut self, action: impl Action + 'static) {
        self.action = Box::new(action)
    }
}

#[cfg(test)]
mod test_default_node {

    use std::sync::Arc;

    use crate::{Content, EnvVar, InChannels, Node, NodeName, NodeTable, OutChannels, Output};

    use super::{Action, DefaultNode};

    use async_trait::async_trait;

    /// An implementation of [`Action`] that returns [`Output::Out`] containing a String "Hello world".
    #[derive(Default)]
    pub struct HelloAction;
    #[async_trait]
    impl Action for HelloAction {
        async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
            Output::Out(Some(Content::new("Hello world".to_string())))
        }
    }

    impl HelloAction {
        pub fn new() -> Self {
            Self::default()
        }
    }

    /// Test for create a default node.
    ///
    /// Step 1: create a [`DefaultNode`] with [`HelloAction`].
    ///
    /// Step 2: run the node and verify its output.
    #[test]
    fn create_default_node() {
        let node_name = "Test Node";

        let mut node_table = NodeTable::new();
        let mut node = DefaultNode::with_action(
            NodeName::from(node_name),
            HelloAction::new(),
            &mut node_table,
        );

        // Check if node table has key-value pair (node.name, node.id)
        assert_eq!(node_table.get(node_name).unwrap(), &node.id());

        let env = Arc::new(EnvVar::new(node_table));
        let out = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { node.run(env).await.get_out().unwrap() });
        let out: &String = out.get().unwrap();
        assert_eq!(out, "Hello world");
    }
}
