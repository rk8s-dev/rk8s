use crate::node::node::NodeId;
use std::collections::{HashMap, HashSet, VecDeque};

/// A simplified graph structure used for cycle detection
pub(crate) struct AbstractGraph {
    /// Maps node IDs to their in-degrees
    pub in_degree: HashMap<NodeId, usize>,
    /// Maps node IDs to their outgoing edges (destination node IDs)
    pub edges: HashMap<NodeId, HashSet<NodeId>>,
    /// Maps folded node IDs to their abstract node IDs
    pub folded_nodes: HashMap<NodeId, NodeId>,
    /// Maps abstract node IDs to concrete node IDs
    pub unfold_abstract_nodes: HashMap<NodeId, Vec<NodeId>>,
}

impl AbstractGraph {
    /// Creates a new empty abstract graph
    pub fn new() -> Self {
        Self {
            in_degree: HashMap::new(),
            edges: HashMap::new(),
            folded_nodes: HashMap::new(),
            unfold_abstract_nodes: HashMap::new(),
        }
    }

    /// Adds a node to the abstract graph
    pub fn add_node(&mut self, node_id: NodeId) {
        if !self.in_degree.contains_key(&node_id) {
            self.in_degree.insert(node_id, 0);
            self.edges.insert(node_id, HashSet::new());
        }
    }

    /// Adds an edge between two nodes in the abstract graph
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        // Look up the abstract node ID that a concrete node ID has been folded into.
        let mut abstract_flag_from = false;
        let mut abstract_flag_to = false;
        let from = if let Some(abstract_id) = self.get_abstract_node_id(&from) {
            abstract_flag_from = true;
            *abstract_id
        } else {
            from
        };
        let to = if let Some(abstract_id) = self.get_abstract_node_id(&to) {
            abstract_flag_to = true;
            *abstract_id
        } else {
            to
        };

        // If both `from` and `to` are abstract node IDs and `from` == `to`, skip the edge addition
        if abstract_flag_from && abstract_flag_to && from == to {
            return;
        }

        log::debug!("Adding edge from {:?} to {:?}", from, to);

        self.edges.get_mut(&from).unwrap().insert(to);
        *self.in_degree.get_mut(&to).unwrap() += 1;
    }

    /// Adds a folded node to the abstract graph
    pub fn add_folded_node(&mut self, abstract_node_id: NodeId, concrete_node_id: Vec<NodeId>) {
        self.add_node(abstract_node_id);

        for concrete_id in &concrete_node_id {
            self.folded_nodes.insert(*concrete_id, abstract_node_id);
        }

        self.unfold_abstract_nodes
            .insert(abstract_node_id, concrete_node_id);
    }

    /// Look up the concrete node IDs that an abstract node ID has been unfolded into.
    pub fn unfold_node(&self, abstract_node_id: NodeId) -> Option<&Vec<NodeId>> {
        self.unfold_abstract_nodes.get(&abstract_node_id)
    }

    /// Look up the abstract node ID that a concrete node ID has been folded into.
    /// Returns None if the node ID has not been folded into an abstract node.
    pub fn get_abstract_node_id(&self, node_id: &NodeId) -> Option<&NodeId> {
        self.folded_nodes.get(node_id)
    }

    /// Returns the total number of nodes in the abstract graph
    pub fn size(&self) -> usize {
        self.in_degree.len()
    }

    /// Check if the graph contains any cycles/loops using a topological sorting approach.
    /// Returns true if the graph contains a cycle, false otherwise.
    pub fn check_loop(&self) -> bool {
        let mut in_degree = self.in_degree.clone();
        let mut visited_count = 0;

        // Start with nodes that have 0 in-degree
        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter_map(|(&node, &degree)| if degree == 0 { Some(node) } else { None })
            .collect();

        while let Some(node) = queue.pop_front() {
            log::debug!("Visiting node: {:?}", node);
            visited_count += 1;

            // For each outgoing edge
            if let Some(nexts) = self.edges.get(&node) {
                // Decrease in-degree of the target node
                for next in nexts {
                    let degree = in_degree.get_mut(next).unwrap();
                    *degree -= 1;
                    // If in-degree becomes 0, add to queue
                    if *degree == 0 {
                        queue.push_back(*next);
                    }
                }
            }
        }

        // If we haven't visited all nodes, there must be a cycle (visited_count != size)
        log::debug!("Visited count: {}, Size: {}", visited_count, self.size());
        visited_count != self.size()
    }
}

#[cfg(test)]
mod abstract_graph_test {
    use super::*;

    /// Tests the cycle detection functionality of the graph.
    /// Creates a graph with two nodes (1 and 2) and adds edges to form a cycle:
    /// Node 1 <-> Node 2
    /// check_loop() returns true.
    #[test]
    fn test_check_loop() {
        let mut graph = AbstractGraph::new();
        graph.add_node(NodeId(1));
        graph.add_node(NodeId(2));
        graph.add_edge(NodeId(1), NodeId(2));
        graph.add_edge(NodeId(2), NodeId(1));
        assert!(graph.check_loop());
    }

    /// Tests the cycle detection functionality of the graph.
    /// Creates a graph with two nodes (1 and 2) and adds a single directed edge:
    /// Node 1 -> Node 2
    /// Since there is no cycle in this graph, check_loop() returns false.
    #[test]
    fn test_check_no_loop() {
        let mut graph = AbstractGraph::new();
        graph.add_node(NodeId(1));
        graph.add_node(NodeId(2));
        graph.add_edge(NodeId(1), NodeId(2));
        assert!(!graph.check_loop());
    }
}
