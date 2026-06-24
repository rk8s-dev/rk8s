// Copyright (C) 2023 Ant Group. All rights reserved.
// 2024 From [fuse_backend_rs](https://github.com/cloud-hypervisor/fuse-backend-rs)
// SPDX-License-Identifier: Apache-2.0

use std::io::{Error, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::HashMap, sync::Arc};

use crate::passthrough::VFS_MAX_INO;

use super::{Inode, OverlayInode};

use futures::future::join_all;
use radix_trie::Trie;
use tracing::{error, trace};

pub struct InodeStore {
    // Active inodes.
    inodes: HashMap<Inode, Arc<OverlayInode>>,
    // Deleted inodes which were unlinked but have non zero lookup count.
    deleted: HashMap<Inode, Arc<OverlayInode>>,
    // Path to inode mapping, used to reserve inode number for same path.
    path_mapping: Trie<String, Inode>,
    // Reserved absolute paths for forgotten inodes (survives FORGET eviction).
    inode_paths: HashMap<Inode, String>,
    next_inode: u64,
    inode_limit: u64,
    // FUSE inode to nlink mapping
    nlinks: HashMap<Inode, Arc<AtomicU64>>,
}

impl InodeStore {
    pub(crate) fn new() -> Self {
        Self {
            inodes: HashMap::new(),
            deleted: HashMap::new(),
            path_mapping: Trie::new(),
            inode_paths: HashMap::new(),
            next_inode: 1,
            inode_limit: VFS_MAX_INO,
            nlinks: HashMap::new(),
        }
    }

    pub(crate) fn alloc_unique_inode(&mut self) -> Result<Inode> {
        // Iter VFS_MAX_INO times to find a free inode number.
        let mut ino = self.next_inode;
        for _ in 0..self.inode_limit {
            if ino > self.inode_limit {
                ino = 1;
            }
            if !self.inodes.contains_key(&ino)
                && !self.deleted.contains_key(&ino)
                && !self.inode_paths.contains_key(&ino)
            {
                self.next_inode = ino + 1;
                return Ok(ino);
            }
            ino += 1;
        }
        error!("reached maximum inode number: {}", self.inode_limit);
        Err(Error::other(format!(
            "maximum inode number {} reached",
            self.inode_limit
        )))
    }

    pub(crate) fn alloc_inode(&mut self, path: &str) -> Result<Inode> {
        match self.path_mapping.get(path) {
            // If the path is already in the mapping, return the reserved inode number.
            Some(v) => Ok(*v),
            // Or allocate and reserve a new inode number before the caller drops the lock.
            None => {
                let inode = self.alloc_unique_inode()?;
                self.path_mapping.insert(path.to_string(), inode);
                self.inode_paths.insert(inode, path.to_string());
                Ok(inode)
            }
        }
    }

    pub(crate) async fn insert_inode(
        &mut self,
        inode: Inode,
        node: Arc<OverlayInode>,
    ) -> Arc<OverlayInode> {
        let path = node.path.read().await.clone();
        let same_active_path = self.path_mapping.get(&path).copied() == Some(inode)
            && self.inodes.contains_key(&inode);
        let old_path = self.inode_paths.get(&inode).cloned();
        let current_nlink = self
            .nlinks
            .get(&inode)
            .map(|nlink| nlink.load(Ordering::Relaxed))
            .unwrap_or(0);

        if let Some(old_path) = old_path.as_deref()
            && old_path != path
            && self.path_mapping.get(old_path).copied() == Some(inode)
        {
            self.path_mapping.remove(old_path);
        }
        self.path_mapping.insert(path.clone(), inode);
        self.inode_paths.insert(inode, path);
        self.deleted.remove(&inode);

        if same_active_path && let Some(existing) = self.inodes.get(&inode).cloned() {
            let real_inodes = node.real_inodes.lock().await.clone();
            *existing.real_inodes.lock().await = real_inodes;
            existing
                .whiteout
                .store(node.whiteout.load(Ordering::Relaxed), Ordering::Relaxed);
            existing
                .loaded
                .store(node.loaded.load(Ordering::Relaxed), Ordering::Relaxed);
            return existing;
        }

        if current_nlink == 0 || (!same_active_path && old_path.is_none()) {
            self.nlinks
                .entry(inode)
                .or_insert_with(|| Arc::new(AtomicU64::new(0)))
                .fetch_add(1, Ordering::Relaxed);
        }
        self.inodes.insert(inode, node.clone());
        node
    }

    pub(crate) fn get_inode(&self, inode: Inode) -> Option<Arc<OverlayInode>> {
        self.inodes.get(&inode).cloned()
    }

    pub(crate) fn get_deleted_inode(&self, inode: Inode) -> Option<Arc<OverlayInode>> {
        self.deleted.get(&inode).cloned()
    }

    /// Reverse lookup for forgotten inodes whose path is still reserved.
    pub(crate) fn path_for_inode(&self, inode: Inode) -> Option<String> {
        self.inode_paths.get(&inode).cloned()
    }

    // Return the inode only if it's permanently deleted from both self.inodes and self.deleted_inodes.
    pub(crate) async fn remove_inode(
        &mut self,
        inode: Inode,
        path_removed: Option<String>,
    ) -> Option<Arc<OverlayInode>> {
        let old_nlink = self.nlinks.get(&inode)?.fetch_sub(1, Ordering::Relaxed);

        if let Some(path) = path_removed {
            self.path_mapping.remove(&path);
            self.inode_paths.remove(&inode);
        }

        if old_nlink == 1
            && let Some(inode_data) = self.inodes.remove(&inode)
        {
            if inode_data.lookups.load(Ordering::Relaxed) > 0 {
                trace!(
                    "InodeStore: inode {inode} unlinked but still in use, moving to deleted map."
                );
                self.deleted.insert(inode, inode_data);
                return None;
            } else {
                trace!("InodeStore: inode {inode} permanently removed (nlink=0, lookups=0).");
                self.nlinks.remove(&inode);
                return Some(inode_data);
            }
        }

        None
    }

    // As a debug function, print all inode numbers in hash table.
    // This function consumes quite lots of memory, so it's disabled by default.
    #[allow(dead_code)]
    pub(crate) async fn debug_print_all_inodes(&self) {
        // Convert the HashMap to Vector<(inode, pathname)>
        let all_inodes_f = self
            .inodes
            .iter()
            .map(|(inode, ovi)| {
                async move {
                    let path = ovi.path.read().await.clone();
                    (inode, path, ovi.lookups.load(Ordering::Relaxed)) // Read the Inode State.
                }
            })
            .collect::<Vec<_>>();
        let mut all_inodes = join_all(all_inodes_f).await;
        all_inodes.sort_by(|a, b| a.0.cmp(b.0));
        trace!("all active inodes: {all_inodes:?}");

        let to_delete = self
            .deleted
            .iter()
            .map(|(inode, ovi)| async move {
                (
                    inode,
                    ovi.path.read().await.clone(),
                    ovi.lookups.load(Ordering::Relaxed),
                )
            })
            .collect::<Vec<_>>();
        let mut delete_to = join_all(to_delete).await;
        delete_to.sort_by(|a, b| a.0.cmp(b.0));
        trace!("all deleted inodes: {delete_to:?}");
    }

    pub fn extend_inode_number(&mut self, next_inode: u64, limit_inode: u64) {
        self.next_inode = next_inode;
        self.inode_limit = limit_inode;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn test_node(path: &str, lookups: u64) -> Arc<OverlayInode> {
        let mut node = OverlayInode::new();
        node.path = tokio::sync::RwLock::new(path.to_string());
        node.name = tokio::sync::RwLock::new(
            path.rsplit('/')
                .find(|component| !component.is_empty())
                .unwrap_or("")
                .to_string(),
        );
        node.lookups = AtomicU64::new(lookups);
        Arc::new(node)
    }

    #[tokio::test]
    async fn test_alloc_unique() {
        let mut store = InodeStore::new();
        let empty_node = Arc::new(OverlayInode::new());
        store.insert_inode(1, empty_node.clone()).await;
        store.insert_inode(2, empty_node.clone()).await;
        store
            .insert_inode(VFS_MAX_INO - 1, empty_node.clone())
            .await;

        let inode = store.alloc_unique_inode().unwrap();
        assert_eq!(inode, 3);
        assert_eq!(store.next_inode, 4);

        store.next_inode = VFS_MAX_INO - 1;
        let inode = store.alloc_unique_inode().unwrap();
        assert_eq!(inode, VFS_MAX_INO);

        let inode = store.alloc_unique_inode().unwrap();
        assert_eq!(inode, 3);
    }

    #[tokio::test]
    async fn test_alloc_existing_path() {
        let mut store = InodeStore::new();
        let mut node_a = OverlayInode::new();
        node_a.path = tokio::sync::RwLock::new("/a".to_string());
        store.insert_inode(1, Arc::new(node_a)).await;
        let mut node_b = OverlayInode::new();
        node_b.path = tokio::sync::RwLock::new("/b".to_string());
        store.insert_inode(2, Arc::new(node_b)).await;
        let mut node_c = OverlayInode::new();
        node_c.path = tokio::sync::RwLock::new("/c".to_string());
        store.insert_inode(VFS_MAX_INO - 1, Arc::new(node_c)).await;

        let inode = store.alloc_inode("/a").unwrap();
        assert_eq!(inode, 1);

        let inode = store.alloc_inode("/b").unwrap();
        assert_eq!(inode, 2);

        let inode = store.alloc_inode("/c").unwrap();
        assert_eq!(inode, VFS_MAX_INO - 1);

        let inode = store.alloc_inode("/notexist").unwrap();
        assert_eq!(inode, 3);
        assert_eq!(store.alloc_inode("/notexist").unwrap(), inode);
        assert_ne!(store.alloc_inode("/other").unwrap(), inode);
    }

    #[tokio::test]
    async fn test_duplicate_insert_same_path_does_not_add_nlink() {
        let mut store = InodeStore::new();
        let inode = store.alloc_inode("/a").unwrap();

        store.insert_inode(inode, test_node("/a", 0)).await;

        store.insert_inode(inode, test_node("/a", 0)).await;

        let removed = store.remove_inode(inode, Some("/a".to_string())).await;
        assert!(removed.is_some());
        assert!(store.get_inode(inode).is_none());
    }

    #[tokio::test]
    async fn test_reinsert_same_inode_updates_reserved_path() {
        let mut store = InodeStore::new();
        let inode = store.alloc_inode("/old").unwrap();
        let node = test_node("/old", 0);
        store.insert_inode(inode, node.clone()).await;

        *node.path.write().await = "/new".to_string();
        store.insert_inode(inode, node).await;

        assert_eq!(store.path_for_inode(inode).as_deref(), Some("/new"));
        assert_eq!(store.alloc_inode("/new").unwrap(), inode);
    }

    #[tokio::test]
    async fn test_reinsert_same_path_keeps_active_node_for_lookup_accounting() {
        let mut store = InodeStore::new();
        let inode = store.alloc_inode("/buck-out/linker_wrapper.sh").unwrap();
        let first = test_node("/buck-out/linker_wrapper.sh", 3);
        let second = test_node("/buck-out/linker_wrapper.sh", 0);

        store.insert_inode(inode, first.clone()).await;
        let active = store.insert_inode(inode, second.clone()).await;

        assert!(
            Arc::ptr_eq(&active, &first),
            "same inode/path reinsertion must keep the active node so pending FORGETs update the right lookup counter"
        );
        assert!(
            !Arc::ptr_eq(&active, &second),
            "same inode/path reinsertion must not swap in a fresh node"
        );
        assert!(Arc::ptr_eq(&active, &store.get_inode(inode).unwrap()));
        assert_eq!(active.lookups.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_rename_reinsert_live_inode_does_not_leave_deleted_alias() {
        let mut store = InodeStore::new();
        let inode = store
            .alloc_inode("/buck-out/tmp-linker_wrapper.sh")
            .unwrap();
        let node = test_node("/buck-out/tmp-linker_wrapper.sh", 1);
        store.insert_inode(inode, node.clone()).await;

        let removed = store
            .remove_inode(inode, Some("/buck-out/tmp-linker_wrapper.sh".to_string()))
            .await;
        assert!(removed.is_none());

        *node.path.write().await = "/buck-out/linker_wrapper.sh".to_string();
        *node.name.write().await = "linker_wrapper.sh".to_string();
        store.insert_inode(inode, node).await;

        assert!(store.get_inode(inode).is_some());
        assert!(
            store.get_deleted_inode(inode).is_none(),
            "renaming a live inode must not leave the same inode in both active and deleted maps"
        );
        assert_eq!(
            store.path_for_inode(inode).as_deref(),
            Some("/buck-out/linker_wrapper.sh")
        );
    }
}
