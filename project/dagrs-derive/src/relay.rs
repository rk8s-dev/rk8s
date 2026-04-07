use std::collections::HashSet;

use proc_macro2::Ident;
use syn::{parse::Parse, Token};

/// Parses and processes a set of relay tasks and their successors, and generates a directed graph.
///
/// Step 1: Define the `Relay` struct with a task and its associated successors (other tasks that depend on it).
///
/// Step 2: Implement the `Parse` trait for `Relaies` to parse a sequence of task-successor pairs from input. This creates a vector of `Relay` objects.
///
/// Step 3: In `add_relay`, initialize a directed graph structure using `Graph` and a hash map to store edges between nodes.
///
/// Step 4: Iterate through each `Relay` and update the graph's edge list by adding nodes (tasks) and defining edges between tasks and their successors.
///
/// Step 5: Ensure that each task is only added once to the graph using a cache (`HashSet`) to avoid duplicates.
///
/// Step 6: Populate the edges of the graph with the previously processed data and return the graph.
///
/// This code provides the logic to dynamically build a graph based on parsed task relationships, where each task is a node and the successors define directed edges between nodes.
pub(crate) struct Relay {
    pub(crate) task: Ident,
    pub(crate) successors: Vec<Ident>,
}

pub(crate) struct Relaies(pub(crate) Vec<Relay>);

impl Parse for Relaies {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut relies = Vec::new();
        loop {
            let mut successors = Vec::new();
            let task = input.parse::<Ident>()?;
            input.parse::<syn::Token!(->)>()?;
            while !input.peek(Token!(,)) && !input.is_empty() {
                successors.push(input.parse::<Ident>()?);
            }
            let relay = Relay { task, successors };
            relies.push(relay);
            let _ = input.parse::<Token!(,)>();
            if input.is_empty() {
                break;
            }
        }
        Ok(Self(relies))
    }
}

pub(crate) fn add_relay(relaies: Relaies) -> proc_macro2::TokenStream {
    let mut token = proc_macro2::TokenStream::new();
    let mut cache: HashSet<Ident> = HashSet::new();
    token.extend(quote::quote!(
        use dagrs::Graph;
        use dagrs::NodeId;
        use std::collections::HashMap;
        use std::collections::HashSet;
        let mut __dagrs_edge_map: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        let mut __dagrs_graph = Graph::new();
    ));
    for relay in relaies.0.iter() {
        let task = relay.task.clone();
        token.extend(quote::quote!(
            let __dagrs_task_id = #task.id();
            if !__dagrs_edge_map.contains_key(&__dagrs_task_id) {
                __dagrs_edge_map.insert(__dagrs_task_id, HashSet::new());
            }
        ));
        for successor in relay.successors.iter() {
            token.extend(quote::quote!(
                let __dagrs_successor_id = #successor.id();
                __dagrs_edge_map.entry(__dagrs_task_id)
                .or_insert_with(HashSet::new)
                .insert(__dagrs_successor_id);
            ));
        }
    }
    for relay in relaies.0.iter() {
        let task = relay.task.clone();
        if !cache.contains(&task) {
            token.extend(quote::quote!(
                __dagrs_graph.add_node(#task)?;
            ));
            cache.insert(task);
        }
        for successor in relay.successors.iter() {
            if !cache.contains(successor) {
                token.extend(quote::quote!(
                    __dagrs_graph.add_node(#successor)?;
                ));
                cache.insert(successor.clone());
            }
        }
    }
    token.extend(quote::quote!(
        for (__dagrs_from_id, __dagrs_to_ids) in &__dagrs_edge_map {
            let __dagrs_targets = __dagrs_to_ids.iter().cloned().collect::<Vec<NodeId>>();
            __dagrs_graph.add_edge(*__dagrs_from_id, __dagrs_targets)?;
        }
    ));

    quote::quote!(
        {
            #token;
            Ok::<dagrs::Graph, dagrs::DagrsError>(__dagrs_graph)
        }
    )
}
