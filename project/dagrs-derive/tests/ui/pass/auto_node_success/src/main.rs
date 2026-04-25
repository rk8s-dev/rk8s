use dagrs_derive::auto_node;
use std::sync::Arc;
use crate::dagrs::Node;

mod dagrs {
    pub mod async_trait {
        pub use ::async_trait::async_trait;
    }

    use std::sync::Arc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct NodeId(pub usize);

    pub type NodeName = String;

    #[derive(Default)]
    pub struct InChannels;

    #[derive(Default)]
    pub struct OutChannels;

    #[derive(Default)]
    pub struct EnvVar;

    #[derive(Clone, Debug, Default)]
    pub struct Output;

    #[async_trait::async_trait]
    pub trait Action: Send + Sync {
        async fn run(
            &self,
            _in_channels: &mut InChannels,
            _out_channels: &mut OutChannels,
            _env: Arc<EnvVar>,
        ) -> Output;
    }

    #[async_trait::async_trait]
    pub trait Node: Send + Sync {
        fn id(&self) -> NodeId;
        fn name(&self) -> NodeName;
        fn input_channels(&mut self) -> &mut InChannels;
        fn output_channels(&mut self) -> &mut OutChannels;
        async fn run(&mut self, env: Arc<EnvVar>) -> Output;
    }
}

struct NoOpAction;

#[dagrs::async_trait::async_trait]
impl dagrs::Action for NoOpAction {
    async fn run(
        &self,
        _: &mut dagrs::InChannels,
        _: &mut dagrs::OutChannels,
        _: Arc<dagrs::EnvVar>,
    ) -> dagrs::Output {
        dagrs::Output
    }
}

#[auto_node]
struct NamedNode {
    label: &'static str,
}

#[auto_node]
struct GenericNode<'a, T>
where
    T: Clone + Send + Sync,
{
    marker: &'a T,
}

#[auto_node]
struct UnitNode;

fn assert_node<T: dagrs::Node>(_node: &T) {}

fn boxed_action() -> Box<dyn dagrs::Action> {
    Box::new(NoOpAction)
}

fn main() {
    let named = NamedNode {
        label: "named",
        id: dagrs::NodeId(1),
        name: "named".to_string(),
        input_channels: dagrs::InChannels,
        output_channels: dagrs::OutChannels,
        action: boxed_action(),
    };
    assert_eq!(named.label, "named");
    assert_eq!(named.id().0, 1);
    assert_node(&named);

    let value = 7usize;
    let generic = GenericNode {
        marker: &value,
        id: dagrs::NodeId(2),
        name: "generic".to_string(),
        input_channels: dagrs::InChannels,
        output_channels: dagrs::OutChannels,
        action: boxed_action(),
    };
    assert_eq!(*generic.marker, 7);
    assert_eq!(generic.name(), "generic".to_string());
    assert_node(&generic);

    let unit = UnitNode {
        id: dagrs::NodeId(3),
        name: "unit".to_string(),
        input_channels: dagrs::InChannels,
        output_channels: dagrs::OutChannels,
        action: boxed_action(),
    };
    assert_eq!(unit.id().0, 3);
    assert_node(&unit);
}
