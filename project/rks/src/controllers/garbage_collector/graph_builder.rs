use std::sync::Arc;

use common::{Finalizer, ObjectMeta, OwnerReference, ResourceKind};
use lru::LruCache;
use tokio::{
    sync::{RwLock, mpsc::UnboundedSender},
    task::JoinHandle,
};

use super::parse_meta_from_yaml;
use crate::controllers::garbage_collector::{
    graph::{Node, UidToNodeTable},
    types::ObjectReference,
};
use crate::{
    api::xlinestore::XlineStore,
    controllers::manager::{ResourceWatchResponse, WatchEvent},
};

#[derive(Debug, Clone)]
pub struct GraphChangeEventInfo {
    pub kind: ResourceKind,
    pub meta: ObjectMeta,
}

#[derive(Debug, Clone)]
pub enum GraphChangeEventKind {
    Add,
    Update,
    Delete,
}
#[derive(Debug, Clone)]
pub struct GraphEvent {
    pub kind: GraphChangeEventKind,
    pub new: GraphChangeEventInfo,
    pub old: Option<GraphChangeEventInfo>,
    pub is_virtual: bool,
}

impl GraphEvent {
    pub fn new_add(new: GraphChangeEventInfo) -> Self {
        Self {
            kind: GraphChangeEventKind::Add,
            new,
            old: None,
            is_virtual: false,
        }
    }

    pub fn new_update(old: GraphChangeEventInfo, new: GraphChangeEventInfo) -> Self {
        Self {
            kind: GraphChangeEventKind::Update,
            new,
            old: Some(old),
            is_virtual: false,
        }
    }

    pub fn new_delete(meta: GraphChangeEventInfo) -> Self {
        Self {
            kind: GraphChangeEventKind::Delete,
            new: meta,
            old: None,
            is_virtual: false,
        }
    }

    pub fn new_virtual_delete(info: GraphChangeEventInfo) -> Self {
        Self {
            kind: GraphChangeEventKind::Delete,
            new: info,
            old: None,
            is_virtual: true,
        }
    }
}

pub struct GraphBuilder {
    xline_store: Arc<XlineStore>,
    pub uid_to_node_table: Arc<UidToNodeTable>,
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    attempt_to_orphan_tx: UnboundedSender<Arc<RwLock<Node>>>,
    pub graph_change_event_tx: Option<UnboundedSender<GraphEvent>>,
    process_task_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub absent_owner_cache: Arc<RwLock<LruCache<ObjectReference, ()>>>,
}

impl GraphBuilder {
    pub fn new(
        xline_store: Arc<XlineStore>,
        attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
        attempt_to_orphan_tx: UnboundedSender<Arc<RwLock<Node>>>,
    ) -> Self {
        Self {
            xline_store,
            uid_to_node_table: Arc::new(UidToNodeTable::default()),
            attempt_to_delete_tx,
            attempt_to_orphan_tx,
            graph_change_event_tx: None,
            process_task_handle: None,
            absent_owner_cache: Arc::new(RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(32).unwrap(),
            ))),
        }
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        // build the initial graph
        let pods = self.xline_store.list_pods().await?;

        for pod in pods {
            // Send add event
            let graph_event = GraphEvent::new_add(GraphChangeEventInfo {
                kind: ResourceKind::Pod,
                meta: pod.metadata.clone(),
            });

            let _ = self
                .graph_change_event_tx
                .as_ref()
                .unwrap()
                .send(graph_event);
        }

        // TODO: handle other resource kinds

        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        self.run_process_graph_change().await;
        self.init().await?;

        Ok(())
    }

    /// Spawn a background task to watch for changes and update the graph accordingly.
    async fn run_process_graph_change(&mut self) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        self.graph_change_event_tx = Some(tx);

        let attempt_to_delete_tx = self.attempt_to_delete_tx.clone();
        let attempt_to_orphan_tx = self.attempt_to_orphan_tx.clone();
        let uid_to_node_table = self.uid_to_node_table.clone();
        let absent_owner_cache = self.absent_owner_cache.clone();

        let handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let new_info = &event.new;

                // Existing a virtual node, update it to real node
                let existing_node = uid_to_node_table.get(&new_info.meta.uid).await;
                if let Some(n) = &existing_node
                    && !event.is_virtual
                    && n.read().await.is_virtual()
                {
                    let existing_node_read_guard = n.read().await;
                    let observed_identity =
                        ObjectReference::from_meta(&new_info.meta, new_info.kind);
                    if &observed_identity != existing_node_read_guard.identity() {
                        // Identity mismatch, find the potential invalid dependents
                        let (_, unmatched_dependents) = partition_dependents_by_identity(
                            existing_node_read_guard.dependents(),
                            &observed_identity,
                        )
                        .await;

                        for unmatched in unmatched_dependents {
                            let read_guard = unmatched.read().await;
                            if read_guard.identity().namespace != observed_identity.namespace {
                                log::info!("[GraphBuilder] An item references an owner but does not match the namespace.
                                            Item: {:?}, Owner: {:?}", read_guard.identity(), observed_identity);
                            }
                            let _ = attempt_to_delete_tx.send(unmatched.clone());
                        }

                        log::info!(
                            "[GraphBuilder] Replacing virtual item with observed item. Virtual: {:?}, Observed: {:?}",
                            existing_node_read_guard.identity(),
                            observed_identity
                        );

                        // clone a new node with observed identity
                        let new_node = Arc::new(RwLock::new(existing_node_read_guard.clone()));

                        new_node.write().await.set_identity(observed_identity);
                        uid_to_node_table
                            .insert(new_info.meta.uid, new_node.clone())
                            .await;
                    }

                    drop(existing_node_read_guard);
                    n.write().await.set_observed();
                }

                match event.kind {
                    GraphChangeEventKind::Add | GraphChangeEventKind::Update => {
                        if let Some(existing_node) = existing_node {
                            let (added_owners, removed_owners, changed_owners) =
                                get_reference_difference(
                                    existing_node.read().await.owners(),
                                    new_info.meta.owner_references.as_deref().unwrap_or(&[]),
                                );

                            if !added_owners.is_empty()
                                || !removed_owners.is_empty()
                                || !changed_owners.is_empty()
                            {
                                // There may be owners that originally be deletion blocked by dependents but no longer do so
                                // because of the update. We need to check these owners to see if we can
                                // delete them now.
                                for removed in &removed_owners {
                                    if removed.block_owner_deletion == Some(true)
                                        && let Some(owner_node) =
                                            uid_to_node_table.get(&removed.uid).await
                                    {
                                        let _ = attempt_to_delete_tx.send(owner_node.clone());
                                    }
                                }
                                for (old_owner, new_owner) in &changed_owners {
                                    if (old_owner.block_owner_deletion == Some(true)
                                        && new_owner.block_owner_deletion == Some(false)
                                        || new_owner.block_owner_deletion.is_none())
                                        && let Some(owner_node) =
                                            uid_to_node_table.get(&new_owner.uid).await
                                    {
                                        let _ = attempt_to_delete_tx.send(owner_node.clone());
                                    }
                                }

                                // Update the owners of the existing node
                                existing_node.write().await.set_owners(
                                    new_info
                                        .meta
                                        .owner_references
                                        .as_deref()
                                        .unwrap_or(&[])
                                        .to_vec(),
                                );

                                // Add to new owners
                                add_dependent_to_owners(
                                    uid_to_node_table.clone(),
                                    attempt_to_delete_tx.clone(),
                                    existing_node.clone(),
                                    &added_owners,
                                )
                                .await;

                                // Remove from removed owners
                                remove_dependent_from_owners(
                                    uid_to_node_table.clone(),
                                    existing_node.clone(),
                                    &removed_owners,
                                )
                                .await;
                            }

                            if being_deleted(&new_info.meta) {
                                existing_node.write().await.set_being_deleted();
                            }

                            process_transition(
                                attempt_to_delete_tx.clone(),
                                attempt_to_orphan_tx.clone(),
                                existing_node.clone(),
                                event.old.as_ref().map(|info| &info.meta),
                                &new_info.meta,
                            )
                            .await;
                        } else {
                            // Node does not exist, create a new one
                            let identity =
                                ObjectReference::from_meta(&new_info.meta, new_info.kind);
                            let new_node = Arc::new(RwLock::new(Node::new(identity, false)));

                            let mut write_guard = new_node.write().await;
                            if being_deleted(&new_info.meta)
                                && has_delete_dependents_finalizer(&new_info.meta)
                            {
                                write_guard.set_deleting_dependents();
                            }
                            if being_deleted(&new_info.meta) {
                                write_guard.set_being_deleted();
                            }
                            drop(write_guard);

                            uid_to_node_table
                                .insert(new_info.meta.uid, new_node.clone())
                                .await;

                            // Process owner references to build relationships
                            if let Some(owners) = &new_info.meta.owner_references {
                                add_dependent_to_owners(
                                    uid_to_node_table.clone(),
                                    attempt_to_delete_tx.clone(),
                                    new_node.clone(),
                                    owners,
                                )
                                .await;
                            }

                            process_transition(
                                attempt_to_delete_tx.clone(),
                                attempt_to_orphan_tx.clone(),
                                new_node,
                                None,
                                &new_info.meta,
                            )
                            .await;
                        }
                    }
                    GraphChangeEventKind::Delete => {
                        if existing_node.is_none() {
                            log::error!(
                                "[GraphBuilder] Delete event for non-existing node with UID: {}",
                                new_info.meta.uid
                            );
                            continue;
                        }

                        let existing_node = existing_node.unwrap();

                        let mut remove_existing_node = true;
                        if event.is_virtual {
                            // this is a virtual delete event from gc
                            let deleted_identity =
                                ObjectReference::from_meta(&new_info.meta, new_info.kind);

                            if existing_node.read().await.is_virtual() {
                                // if the existing node is also virtual
                                // check if there are dependents that reference existing node with
                                // other identity (e.g., name or kind mismatch)
                                let (matching_dependents, unmatching_dependents) =
                                    partition_dependents_by_identity(
                                        existing_node.read().await.dependents(),
                                        &deleted_identity,
                                    )
                                    .await;

                                if !unmatching_dependents.is_empty() {
                                    // there are still dependents that reference existing node with other identity
                                    // so we cannot remove the existing node
                                    remove_existing_node = false;

                                    if !matching_dependents.is_empty() {
                                        absent_owner_cache
                                            .write()
                                            .await
                                            .put(deleted_identity.clone(), ());
                                        for dependent in matching_dependents {
                                            let _ = attempt_to_delete_tx.send(dependent.clone());
                                        }
                                    }

                                    // if the virtual delete event verifies that existing node's identity does not exist
                                    if existing_node.read().await.identity() == &deleted_identity {
                                        // get an alternative identity for unmatching dependents to reference
                                        let alternative_identity = unmatching_dependents
                                            .first()
                                            .unwrap()
                                            .read()
                                            .await
                                            .owners()
                                            .iter()
                                            .find(|owner| owner.uid == deleted_identity.uid)
                                            .map(|owner| {
                                                ObjectReference::from_owner_reference(
                                                    owner,
                                                    deleted_identity.namespace.clone(),
                                                )
                                            });

                                        if let Some(alternative_identity) = alternative_identity {
                                            let node = existing_node.read().await;
                                            let replacement_node =
                                                Arc::new(RwLock::new(node.clone()));
                                            uid_to_node_table
                                                .insert(
                                                    alternative_identity.uid,
                                                    replacement_node.clone(),
                                                )
                                                .await;
                                            // add to attempt to delete channel to check the replacement virtual node
                                            let _ = attempt_to_delete_tx.send(replacement_node);
                                        }
                                    }
                                }
                            } else if existing_node.read().await.identity() != &deleted_identity {
                                // virtual delete event but existing node is real
                                // and identity mismatch, cannot remove existing node by virtual delete event
                                remove_existing_node = false;

                                let (matching_dependents, _) = partition_dependents_by_identity(
                                    existing_node.read().await.dependents(),
                                    &deleted_identity,
                                )
                                .await;
                                if !matching_dependents.is_empty() {
                                    absent_owner_cache
                                        .write()
                                        .await
                                        .put(deleted_identity.clone(), ());
                                    for dependent in matching_dependents {
                                        let _ = attempt_to_delete_tx.send(dependent.clone());
                                    }
                                }
                            }
                        }

                        if !remove_existing_node {
                            continue;
                        }

                        uid_to_node_table.remove(&new_info.meta.uid).await;

                        // Capture owners before mutating node relationships so we can requeue them later.
                        let owners_snapshot = existing_node.read().await.owners().clone();

                        // Remove the node from its owners' dependents
                        remove_dependent_from_owners(
                            uid_to_node_table.clone(),
                            existing_node.clone(),
                            &owners_snapshot,
                        )
                        .await;

                        if existing_node.read().await.dependents_length() > 0 {
                            absent_owner_cache.write().await.put(
                                ObjectReference::from_meta(&new_info.meta, new_info.kind),
                                (),
                            );
                        }

                        for dependent in existing_node.read().await.dependents() {
                            let _ = attempt_to_delete_tx.send(dependent.clone());
                        }

                        // let gc check if all the owner's
                        // dependents are deleted, if so, the owner will be deleted.
                        for owner in &owners_snapshot {
                            if let Some(owner_node) = uid_to_node_table.get(&owner.uid).await
                                && owner_node.read().await.is_deleting_dependents()
                            {
                                let _ = attempt_to_delete_tx.send(owner_node.clone());
                            }
                        }
                    }
                }
            }
            Ok(())
        });
        self.process_task_handle = Some(handle);
    }
}

pub async fn handle_watch_resp(
    resp: &ResourceWatchResponse,
    graph_change_event_tx: UnboundedSender<GraphEvent>,
) -> anyhow::Result<()> {
    let kind = resp.kind;
    match &resp.event {
        WatchEvent::Add { yaml } => {
            let graph_event = GraphEvent::new_add(GraphChangeEventInfo {
                kind,
                meta: parse_meta_from_yaml(yaml)?,
            });
            let _ = graph_change_event_tx.send(graph_event);
        }
        WatchEvent::Update { old_yaml, new_yaml } => {
            let graph_event = GraphEvent::new_update(
                GraphChangeEventInfo {
                    kind,
                    meta: parse_meta_from_yaml(old_yaml)?,
                },
                GraphChangeEventInfo {
                    kind,
                    meta: parse_meta_from_yaml(new_yaml)?,
                },
            );
            let _ = graph_change_event_tx.send(graph_event);
        }
        WatchEvent::Delete { yaml } => {
            let graph_event = GraphEvent::new_delete(GraphChangeEventInfo {
                kind,
                meta: parse_meta_from_yaml(yaml)?,
            });
            let _ = graph_change_event_tx.send(graph_event);
        }
    }
    Ok(())
}

/// Add a dependent node to its owner nodes based on the provided owner references.
pub async fn add_dependent_to_owners(
    uid_to_node_table: Arc<UidToNodeTable>,
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    node: Arc<RwLock<Node>>,
    owners: &[OwnerReference],
) {
    for owner in owners {
        let owner_node = if let Some(n) = uid_to_node_table.get(&owner.uid).await {
            n
        } else {
            let owner_ref = ObjectReference::from_owner_reference(
                owner,
                node.read().await.identity().namespace.clone(),
            );

            let virtual_node = Arc::new(RwLock::new(Node::new(owner_ref, true)));
            let uid = virtual_node.read().await.identity().uid;
            uid_to_node_table.insert(uid, virtual_node.clone()).await;
            let _ = attempt_to_delete_tx.send(virtual_node.clone());
            virtual_node
        };

        node.write().await.owners_mut().push(owner.clone());
        owner_node.write().await.add_dependent(node.clone());
    }
}

/// Remove a dependent node from its owner nodes based on the provided owner references.
pub async fn remove_dependent_from_owners(
    uid_to_node_table: Arc<UidToNodeTable>,
    node: Arc<RwLock<Node>>,
    owners: &[OwnerReference],
) {
    for owner in owners {
        if let Some(owner_node) = uid_to_node_table.get(&owner.uid).await {
            owner_node.write().await.remove_dependent(node.clone());
        }
        node.write()
            .await
            .owners_mut()
            .retain(|o| o.uid != owner.uid);
    }
}

/// Get the difference between old and new owner references.
/// Returns a tuple of (added_owners, removed_owners, changed_owners).
pub fn get_reference_difference(
    old_owners: &[OwnerReference],
    new_owners: &[OwnerReference],
) -> (
    Vec<OwnerReference>,
    Vec<OwnerReference>,
    Vec<(OwnerReference, OwnerReference)>,
) {
    let mut added_owners = Vec::new();
    let mut removed_owners = Vec::new();
    let mut changed_owners = Vec::new();

    for new_owner in new_owners {
        if !old_owners.iter().any(|o| o.uid == new_owner.uid) {
            added_owners.push(new_owner.clone());
        } else {
            let old_owner = old_owners.iter().find(|o| o.uid == new_owner.uid).unwrap();
            if old_owner.block_owner_deletion != new_owner.block_owner_deletion {
                changed_owners.push((old_owner.clone(), new_owner.clone()));
            }
        }
    }

    for old_owner in old_owners {
        if !new_owners.iter().any(|o| o.uid == old_owner.uid) {
            removed_owners.push(old_owner.clone());
        }
    }

    (added_owners, removed_owners, changed_owners)
}

/// Partition dependents into matched and unmatched based on identity.
async fn partition_dependents_by_identity(
    dependents: &Vec<Arc<RwLock<Node>>>,
    identity: &ObjectReference,
) -> (Vec<Arc<RwLock<Node>>>, Vec<Arc<RwLock<Node>>>) {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    for dependent in dependents {
        let read_guard = dependent.read().await;
        // Check if any owner matches the identity
        let has_matching_owner = read_guard.owners().iter().any(|owner| {
            owner.uid == identity.uid && owner.name == identity.name && owner.kind == identity.kind
        });

        if has_matching_owner {
            matched.push(dependent.clone());
        } else {
            unmatched.push(dependent.clone());
        }
    }
    (matched, unmatched)
}

pub fn being_deleted(meta: &ObjectMeta) -> bool {
    meta.deletion_timestamp.is_some()
}

pub fn has_delete_dependents_finalizer(meta: &ObjectMeta) -> bool {
    has_finalizer(meta, &Finalizer::DeletingDependents)
}

pub fn has_orphan_dependents_finalizer(meta: &ObjectMeta) -> bool {
    has_finalizer(meta, &Finalizer::OrphanDependents)
}

fn has_finalizer(meta: &ObjectMeta, finalizer: &Finalizer) -> bool {
    if let Some(finalizers) = &meta.finalizers {
        finalizers.iter().any(|f| f == finalizer)
    } else {
        false
    }
}

fn deletion_starts_with_finalizer(
    old_meta: Option<&ObjectMeta>,
    new_meta: &ObjectMeta,
    finalizer: &Finalizer,
) -> bool {
    // First, check the new object's status.
    // if new object is not in a deleting state or does not have the finalizer we interested in,
    // that means there is no deletion starts with finalizer
    if !being_deleted(new_meta) || !has_finalizer(new_meta, finalizer) {
        return false;
    }

    // Then, check the old object's status.
    if old_meta.is_none() {
        // If old object is None, it means the object is newly created with deletion timestamp and finalizer
        // which means that the newly created object is already in deleting state with finalizer
        return true;
    }
    // If old object is not None and new object is in deleting state with finalizer,
    // we need to check the old object to see if there has a transition between old and new.
    !being_deleted(old_meta.unwrap()) || !has_finalizer(old_meta.unwrap(), finalizer)
}

async fn process_transition(
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    attempt_to_orphan_tx: UnboundedSender<Arc<RwLock<Node>>>,
    node: Arc<RwLock<Node>>,
    old_meta: Option<&ObjectMeta>,
    new_meta: &ObjectMeta,
) {
    if deletion_starts_with_finalizer(old_meta, new_meta, &Finalizer::OrphanDependents) {
        let _ = attempt_to_orphan_tx.send(node);
        return;
    }

    if deletion_starts_with_finalizer(old_meta, new_meta, &Finalizer::DeletingDependents) {
        node.write().await.set_deleting_dependents();
        for dependent in node.read().await.dependents() {
            let _ = attempt_to_delete_tx.send(dependent.clone());
        }
        let _ = attempt_to_delete_tx.send(node);
    }
}

impl Drop for GraphBuilder {
    fn drop(&mut self) {
        if let Some(handle) = self.process_task_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_add_dependent_to_owners() {
        let uid_to_node_table = Arc::new(UidToNodeTable::default());
        let (attempt_to_delete_tx, mut attempt_to_delete_rx) =
            tokio::sync::mpsc::unbounded_channel();

        let owner_ref = OwnerReference {
            api_version: "v1".to_string(),
            kind: "Deployment".into(),
            name: "deploy1".to_string(),
            uid: Uuid::new_v4(),
            controller: true,
            block_owner_deletion: Some(false),
        };

        let node = Arc::new(RwLock::new(Node::new(
            ObjectReference::new(
                ResourceKind::Deployment,
                "deploy1".to_string(),
                owner_ref.uid,
                "default".to_string(),
                Some(false),
            ),
            false,
        )));

        uid_to_node_table.insert(owner_ref.uid, node.clone()).await;

        let owner_ref2 = OwnerReference {
            api_version: "v1".to_string(),
            kind: "Service".into(),
            name: "svc1".to_string(),
            uid: Uuid::new_v4(),
            controller: false,
            block_owner_deletion: Some(false),
        };

        let object_ref = ObjectReference::new(
            ResourceKind::Pod,
            "pod1".to_string(),
            Uuid::new_v4(),
            "default".to_string(),
            Some(true),
        );

        let node = Arc::new(RwLock::new(Node::new(object_ref, false)));

        add_dependent_to_owners(
            uid_to_node_table.clone(),
            attempt_to_delete_tx.clone(),
            node.clone(),
            &[owner_ref.clone(), owner_ref2.clone()],
        )
        .await;

        let owner_node = uid_to_node_table.get(&owner_ref.uid).await;
        assert!(owner_node.is_some());
        let owner_node = owner_node.unwrap();
        let owner_node_guard = owner_node.read().await;
        let dependents = owner_node_guard.dependents();
        assert_eq!(dependents.len(), 1);
        let dependent_guard = dependents[0].read().await;
        assert_eq!(dependent_guard.identity().name, "pod1");
        assert!(attempt_to_delete_rx.try_recv().is_ok());
        assert!(attempt_to_delete_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_get_reference_difference() {
        let old_owners = vec![
            OwnerReference {
                api_version: "v1".to_string(),
                kind: "Deployment".into(),
                name: "deploy1".to_string(),
                uid: Uuid::new_v4(),
                controller: true,
                block_owner_deletion: Some(true),
            },
            OwnerReference {
                api_version: "v1".to_string(),
                kind: "Service".into(),
                name: "svc1".to_string(),
                uid: Uuid::new_v4(),
                controller: false,
                block_owner_deletion: Some(false),
            },
        ];

        let new_owners = vec![
            OwnerReference {
                api_version: "v1".to_string(),
                kind: "Deployment".into(),
                name: "deploy1".to_string(),
                uid: old_owners[0].uid,
                controller: true,
                block_owner_deletion: Some(false),
            },
            OwnerReference {
                api_version: "v1".to_string(),
                kind: "Service".into(),
                name: "svc2".to_string(),
                uid: Uuid::new_v4(),
                controller: false,
                block_owner_deletion: Some(true),
            },
        ];
        let (added, removed, changed) = get_reference_difference(&old_owners, &new_owners);
        assert_eq!(added.len(), 1);
        assert_eq!(removed.len(), 1);
        assert_eq!(changed.len(), 1);

        assert_eq!(added[0].name, "svc2");
        assert_eq!(removed[0].name, "svc1");
        assert_eq!(changed[0].0.name, "deploy1");
        assert_eq!(changed[0].1.name, "deploy1");
        assert_eq!(changed[0].0.block_owner_deletion, Some(true));
        assert_eq!(changed[0].1.block_owner_deletion, Some(false));
    }

    #[tokio::test]
    async fn test_remove_dependent_from_owners() {
        let uid_to_node_table = Arc::new(UidToNodeTable::default());

        let owner_ref = OwnerReference {
            api_version: "v1".to_string(),
            kind: "Deployment".into(),
            name: "deploy1".to_string(),
            uid: Uuid::new_v4(),
            controller: true,
            block_owner_deletion: Some(false),
        };

        let node = Arc::new(RwLock::new(Node::new(
            ObjectReference::new(
                ResourceKind::Deployment,
                "deploy1".to_string(),
                owner_ref.uid,
                "default".to_string(),
                Some(false),
            ),
            false,
        )));

        uid_to_node_table.insert(owner_ref.uid, node.clone()).await;

        let object_ref = ObjectReference::new(
            ResourceKind::Pod,
            "pod1".to_string(),
            Uuid::new_v4(),
            "default".to_string(),
            Some(true),
        );

        let node = Arc::new(RwLock::new(Node::new(object_ref, false)));

        // First, add the dependent
        add_dependent_to_owners(
            uid_to_node_table.clone(),
            tokio::sync::mpsc::unbounded_channel().0,
            node.clone(),
            std::slice::from_ref(&owner_ref),
        )
        .await;

        // Now, remove the dependent
        remove_dependent_from_owners(
            uid_to_node_table.clone(),
            node.clone(),
            std::slice::from_ref(&owner_ref),
        )
        .await;

        let owner_node = uid_to_node_table.get(&owner_ref.uid).await;
        assert!(owner_node.is_some());
        let owner_node = owner_node.unwrap();
        let owner_node_guard = owner_node.read().await;
        let dependents = owner_node_guard.dependents();
        assert_eq!(dependents.len(), 0);
    }
}
