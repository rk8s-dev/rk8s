use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output, ResetPolicy};
use std::sync::Arc;

const TEST_KEY: &str = "reset-policy-value";

struct ReadEnvAction;

#[async_trait]
impl Action for ReadEnvAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, env: Arc<EnvVar>) -> Output {
        let value = env
            .get::<String>(TEST_KEY)
            .unwrap_or_else(|| "missing".to_string());
        Output::new(value)
    }
}

#[tokio::test]
async fn test_reset_policy_controls_environment_lifecycle() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let node = DefaultNode::with_action("EnvReader".to_string(), ReadEnvAction, &mut table);
    let node_id = node.id();
    graph.add_node(node).unwrap();

    let mut env = EnvVar::new(NodeTable::default());
    env.set(TEST_KEY, "configured".to_string());
    graph.set_env(env);

    graph.async_start().await.unwrap();
    assert_eq!(
        graph.get_results::<String>()[&node_id]
            .as_ref()
            .unwrap()
            .as_ref(),
        "configured"
    );

    graph.reset().await.unwrap();
    graph.async_start().await.unwrap();
    assert_eq!(
        graph.get_results::<String>()[&node_id]
            .as_ref()
            .unwrap()
            .as_ref(),
        "configured"
    );

    graph.reset_with(ResetPolicy::ResetEnv).await.unwrap();
    graph.async_start().await.unwrap();
    assert_eq!(
        graph.get_results::<String>()[&node_id]
            .as_ref()
            .unwrap()
            .as_ref(),
        "missing"
    );
}
