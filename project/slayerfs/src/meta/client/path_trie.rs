use std::collections::HashMap;
use std::sync::Arc;

use async_recursion::async_recursion;
use tokio::sync::RwLock;

struct TrieNode {
    children: HashMap<String, Arc<RwLock<TrieNode>>>,
    inodes: Vec<i64>,
    is_terminal: bool,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            inodes: Vec::new(),
            is_terminal: false,
        }
    }
}

/// Path Trie for efficient path-to-inode mapping and prefix-based invalidation
pub(crate) struct PathTrie {
    root: Arc<RwLock<TrieNode>>,
}

impl PathTrie {
    pub(crate) fn new() -> Self {
        Self {
            root: Arc::new(RwLock::new(TrieNode::new())),
        }
    }

    pub(crate) async fn insert(&self, path: &str, ino: i64) {
        let components = Self::split_path(path);
        let mut current = self.root.clone();

        for component in components {
            let mut node = current.write().await;
            let next = node
                .children
                .entry(component.to_string())
                .or_insert_with(|| Arc::new(RwLock::new(TrieNode::new())))
                .clone();
            drop(node);
            current = next;
        }

        let mut terminal_node = current.write().await;
        terminal_node.is_terminal = true;
        if !terminal_node.inodes.contains(&ino) {
            terminal_node.inodes.push(ino);
        }
    }

    pub(crate) async fn remove_by_prefix(&self, path: &str) -> Vec<(String, Vec<i64>)> {
        let components = Self::split_path(path);

        if components.is_empty() {
            let mut root = self.root.write().await;
            let removed = Self::collect_all_paths_from_node(&root, "").await;
            root.children.clear();
            root.inodes.clear();
            root.is_terminal = false;
            return removed;
        }

        let mut current = self.root.clone();
        for (i, component) in components.iter().enumerate() {
            let node = current.read().await;

            if i == components.len() - 1 {
                let child = node.children.get(*component).cloned();
                drop(node);

                if let Some(child_node) = child {
                    let child_guard = child_node.read().await;
                    let removed = Self::collect_all_paths_from_node(&child_guard, path).await;
                    drop(child_guard);

                    let mut parent = current.write().await;
                    parent.children.remove(*component);

                    return removed;
                }
                return Vec::new();
            }

            let next = node.children.get(*component).cloned();
            drop(node);

            if let Some(next_node) = next {
                current = next_node;
            } else {
                return Vec::new();
            }
        }

        Vec::new()
    }

    #[async_recursion]
    async fn collect_all_paths_from_node(node: &TrieNode, prefix: &str) -> Vec<(String, Vec<i64>)> {
        let mut paths = Vec::new();

        if node.is_terminal {
            paths.push((prefix.to_string(), node.inodes.clone()));
        }

        for (component, child_arc) in &node.children {
            let child = child_arc.read().await;
            let child_prefix = if prefix.is_empty() {
                format!("/{}", component)
            } else {
                format!("{}/{}", prefix, component)
            };

            let child_paths = Self::collect_all_paths_from_node(&child, &child_prefix).await;
            paths.extend(child_paths);
        }

        paths
    }

    fn split_path(path: &str) -> Vec<&str> {
        path.trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    }
}
