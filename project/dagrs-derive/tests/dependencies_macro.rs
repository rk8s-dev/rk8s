use crate::dagrs::Node;
use dagrs_derive::dependencies;

mod dagrs {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct NodeId(pub usize);

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DagrsError(pub &'static str);

    pub type DagrsResult<T> = Result<T, DagrsError>;

    pub trait Node {
        fn id(&self) -> NodeId;
    }

    #[derive(Debug, Default)]
    pub struct Graph {
        pub nodes: Vec<NodeId>,
        pub edges: Vec<(NodeId, Vec<NodeId>)>,
    }

    impl Graph {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_node(&mut self, node: impl Node + 'static) -> DagrsResult<NodeId> {
            let node_id = node.id();
            if self.nodes.contains(&node_id) {
                return Err(DagrsError("duplicate node id"));
            }
            self.nodes.push(node_id);
            Ok(node_id)
        }

        pub fn add_edge(&mut self, from: NodeId, to: Vec<NodeId>) -> DagrsResult<()> {
            if !self.nodes.contains(&from) {
                return Err(DagrsError("missing source node"));
            }
            self.edges.push((from, to));
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
struct TestNode {
    id: dagrs::NodeId,
}

impl dagrs::Node for TestNode {
    fn id(&self) -> dagrs::NodeId {
        self.id
    }
}

#[test]
fn dependencies_macro_supports_question_mark() -> dagrs::DagrsResult<()> {
    let a = TestNode {
        id: dagrs::NodeId(1),
    };
    let b = TestNode {
        id: dagrs::NodeId(2),
    };
    let c = TestNode {
        id: dagrs::NodeId(3),
    };

    let graph = dependencies!(a -> b c, b -> c)?;
    assert_eq!(
        graph.nodes,
        vec![dagrs::NodeId(1), dagrs::NodeId(2), dagrs::NodeId(3)]
    );
    assert_eq!(graph.edges.len(), 2);
    assert!(graph.edges.iter().any(|(from, to)| {
        *from == dagrs::NodeId(1)
            && to.contains(&dagrs::NodeId(2))
            && to.contains(&dagrs::NodeId(3))
    }));
    assert!(graph
        .edges
        .iter()
        .any(|(from, to)| { *from == dagrs::NodeId(2) && to == &vec![dagrs::NodeId(3)] }));
    Ok(())
}

#[test]
fn dependencies_macro_propagates_graph_build_errors() {
    let a = TestNode {
        id: dagrs::NodeId(7),
    };
    let b = TestNode {
        id: dagrs::NodeId(7),
    };

    let err = (|| -> dagrs::DagrsResult<dagrs::Graph> { dependencies!(a -> b) })()
        .expect_err("duplicate node ids should fail");
    assert_eq!(err, dagrs::DagrsError("duplicate node id"));
}

#[test]
fn dependencies_macro_handles_internal_identifier_collisions() -> dagrs::DagrsResult<()> {
    let graph = TestNode {
        id: dagrs::NodeId(11),
    };
    let edge = TestNode {
        id: dagrs::NodeId(12),
    };
    let vec = TestNode {
        id: dagrs::NodeId(13),
    };

    let built = dependencies!(graph -> edge vec, edge -> vec)?;

    assert_eq!(
        built.nodes,
        vec![dagrs::NodeId(11), dagrs::NodeId(12), dagrs::NodeId(13)]
    );
    assert_eq!(built.edges.len(), 2);
    Ok(())
}
