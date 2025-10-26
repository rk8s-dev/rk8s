use std::sync::Arc;

use common::{DeletePropagationPolicy, Finalizer, ObjectMeta, OwnerReference, ResourceKind};
use lru::LruCache;
use tokio::sync::{
    Mutex, RwLock,
    mpsc::{UnboundedReceiver, UnboundedSender},
};

use self::{
    graph::Node,
    graph_builder::{
        GraphBuilder, GraphChangeEventInfo, GraphEvent, has_delete_dependents_finalizer,
        has_orphan_dependents_finalizer,
    },
    types::ObjectReference,
};
use crate::{
    api::xlinestore::XlineStore,
    controllers::{Controller, manager::ResourceWatchResponse},
};

pub mod graph;
pub mod graph_builder;
pub mod types;

#[allow(unused)]
pub struct GarbageCollector {
    xline_store: Arc<XlineStore>,
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    attempt_to_delete_rx: Arc<Mutex<UnboundedReceiver<Arc<RwLock<Node>>>>>,
    attempt_to_orphan_tx: UnboundedSender<Arc<RwLock<Node>>>,
    attempt_to_orphan_rx: Arc<Mutex<UnboundedReceiver<Arc<RwLock<Node>>>>>,
    pub dependency_graph_builder: GraphBuilder,
}

#[async_trait::async_trait]
impl Controller for GarbageCollector {
    fn name(&self) -> &'static str {
        "GarbageCollector"
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        self.dependency_graph_builder.run().await?;

        self.run_process_attempt_to_delete(4).await;
        self.run_process_attempt_to_orphan(4).await;
        Ok(())
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::Pod, ResourceKind::ReplicaSet]
    }

    async fn handle_watch_response(
        &mut self,
        response: &ResourceWatchResponse,
    ) -> anyhow::Result<()> {
        if let Some(graph_change_event_tx) =
            self.dependency_graph_builder.graph_change_event_tx.clone()
        {
            graph_builder::handle_watch_resp(response, graph_change_event_tx).await
        } else {
            Err(anyhow::anyhow!(
                "[GarbageCollector] Graph change event tx is not set"
            ))
        }
    }
}

impl GarbageCollector {
    pub fn new(xline_store: Arc<XlineStore>) -> Self {
        let (attempt_to_delete_tx, attempt_to_delete_rx) = tokio::sync::mpsc::unbounded_channel();
        let (attempt_to_orphan_tx, attempt_to_orphan_rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            xline_store: xline_store.clone(),
            attempt_to_delete_tx: attempt_to_delete_tx.clone(),
            attempt_to_delete_rx: Arc::new(Mutex::new(attempt_to_delete_rx)),
            attempt_to_orphan_tx: attempt_to_orphan_tx.clone(),
            attempt_to_orphan_rx: Arc::new(Mutex::new(attempt_to_orphan_rx)),
            dependency_graph_builder: GraphBuilder::new(
                xline_store,
                attempt_to_delete_tx,
                attempt_to_orphan_tx,
            ),
        }
    }

    async fn run_process_attempt_to_delete(&mut self, num_workers: usize) {
        for i in 0..num_workers {
            let attempt_to_delete_rx = self.attempt_to_delete_rx.clone();
            let attempt_to_delete_tx = self.attempt_to_delete_tx.clone();
            let xline_store = self.xline_store.clone();

            let uid_to_node_table = self.dependency_graph_builder.uid_to_node_table.clone();
            let absent_owner_cache = self.dependency_graph_builder.absent_owner_cache.clone();
            let graph_change_event_tx = self
                .dependency_graph_builder
                .graph_change_event_tx
                .clone()
                .unwrap();
            tokio::spawn(async move {
                log::debug!(
                    "[GarbageCollector] Starting process_attempt_to_delete task [#{}]",
                    i
                );
                while let Some(node) = attempt_to_delete_rx.lock().await.recv().await {
                    if !node.read().await.is_observed() {
                        if let Some(existing_node) = uid_to_node_table
                            .get(&node.read().await.identity().uid)
                            .await
                        {
                            if existing_node.read().await.is_observed() {
                                // Node has been observed; skip deletion
                                continue;
                            }
                        } else {
                            // Node no longer exists; skip deletion
                            continue;
                        }
                    }
                    match attempt_to_delete_item(
                        xline_store.clone(),
                        attempt_to_delete_tx.clone(),
                        graph_change_event_tx.clone(),
                        absent_owner_cache.clone(),
                        node.clone(),
                    )
                    .await
                    {
                        Ok(_) => {
                            if !node.read().await.is_observed() {
                                let _ = attempt_to_delete_tx.send(node.clone());
                            }
                        }
                        Err(AttemptToDeleteItemError::SentVirtualDeleteEvent) => {}
                        Err(e) => {
                            log::error!(
                                "[GarbageCollector] Error attempting to delete item {}: {}",
                                node.read().await.identity().name,
                                e
                            );

                            // Re-enqueue the node for another attempt
                            let _ = attempt_to_delete_tx.send(node.clone());
                        }
                    }
                }
            });
        }
    }

    async fn run_process_attempt_to_orphan(&mut self, num_workers: usize) {
        for i in 0..num_workers {
            let attempt_to_orphan_rx = self.attempt_to_orphan_rx.clone();
            let attempt_to_orphan_tx = self.attempt_to_orphan_tx.clone();
            let xline_store = self.xline_store.clone();

            tokio::spawn(async move {
                log::debug!(
                    "[GarbageCollector] Starting process_attempt_to_orphan task [#{}]",
                    i
                );
                while let Some(node) = attempt_to_orphan_rx.lock().await.recv().await {
                    match attempt_to_orphan_item(&xline_store, &node).await {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!(
                                "[GarbageCollector] Error attempting to orphan item {}: {}",
                                node.read().await.identity().name,
                                e
                            );

                            // Re-enqueue the node for another attempt
                            let _ = attempt_to_orphan_tx.send(node.clone());
                        }
                    }
                }
            });
        }
    }
}

async fn attempt_to_orphan_item(
    xline_store: &XlineStore,
    owner: &RwLock<Node>,
) -> anyhow::Result<()> {
    let read_guard = owner.read().await;

    orphan_dependents(xline_store, read_guard.identity(), read_guard.dependents()).await?;

    remove_finalizer(xline_store, owner, Finalizer::OrphanDependents).await?;
    Ok(())
}

async fn orphan_dependents(
    xline_store: &XlineStore,
    identity: &ObjectReference,
    dependents: &Vec<Arc<RwLock<Node>>>,
) -> anyhow::Result<()> {
    for dependent in dependents {
        let mut owner_refs = dependent.read().await.owners().clone();
        owner_refs.retain(|owner_ref| owner_ref.uid != identity.uid);

        let origin_yaml = get_object_yaml(xline_store, dependent.read().await.identity()).await?;

        if let Some(yaml) = origin_yaml {
            patch_new_owner_references(xline_store, dependent, &yaml, owner_refs).await?;
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum AttemptToDeleteItemError {
    #[error("Sent virtual delete event")]
    SentVirtualDeleteEvent,
    #[error("Other error occurred")]
    Other(anyhow::Error),
}

impl From<anyhow::Error> for AttemptToDeleteItemError {
    fn from(err: anyhow::Error) -> Self {
        AttemptToDeleteItemError::Other(err)
    }
}

async fn attempt_to_delete_item(
    xline_store: Arc<XlineStore>,
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    graph_change_event_tx: UnboundedSender<GraphEvent>,
    absent_owner_cache: Arc<RwLock<LruCache<ObjectReference, ()>>>,
    node: Arc<RwLock<Node>>,
) -> Result<(), AttemptToDeleteItemError> {
    log::debug!(
        "[Garbage Collector] Attempting to delete item: {}",
        node.read().await.identity().name
    );

    let (
        identity,
        is_being_deleted,
        is_deleting_dependents,
        dependents_snapshot,
        dependents_length,
    ) = {
        let guard = node.read().await;
        (
            guard.identity().clone(),
            guard.is_being_deleted(),
            guard.is_deleting_dependents(),
            guard.dependents().clone(),
            guard.dependents_length(),
        )
    };

    if is_being_deleted && !is_deleting_dependents {
        return Ok(());
    }

    let obj_yaml = get_object_yaml(xline_store.as_ref(), &identity).await?;

    if obj_yaml.is_none() {
        // Object does not exist in xline, send virtual delete event to graph builder
        let _ = graph_change_event_tx.send(GraphEvent::new_virtual_delete(GraphChangeEventInfo {
            kind: identity.kind,
            meta: ObjectMeta {
                name: identity.name.clone(),
                namespace: identity.namespace.clone(),
                uid: identity.uid,
                ..Default::default()
            },
        }));

        return Err(AttemptToDeleteItemError::SentVirtualDeleteEvent);
    }
    let obj_yaml = obj_yaml.unwrap();

    let meta = parse_meta_from_yaml(&obj_yaml)?;
    if meta.uid != identity.uid {
        // UID mismatch, send virtual delete event to graph builder
        let _ = graph_change_event_tx.send(GraphEvent::new_virtual_delete(GraphChangeEventInfo {
            kind: identity.kind,
            meta: ObjectMeta {
                name: identity.name.clone(),
                namespace: identity.namespace.clone(),
                uid: identity.uid,
                ..Default::default()
            },
        }));

        return Err(AttemptToDeleteItemError::SentVirtualDeleteEvent);
    }

    if is_deleting_dependents {
        return process_deleting_dependents(
            xline_store,
            attempt_to_delete_tx.clone(),
            node.clone(),
        )
        .await;
    }

    let owner_refs = meta.owner_references.clone();
    if owner_refs.is_none() {
        // No owners, no action needed
        return Ok(());
    }

    let owners = owner_refs.unwrap();

    let (solid, dangling, waiting_for_dependents_deletion) = classify_owners(
        &owners,
        &node,
        xline_store.as_ref(),
        absent_owner_cache.clone(),
    )
    .await?;

    if !solid.is_empty() {
        log::debug!("Solid owners exist , will not delete");

        if dangling.is_empty() && waiting_for_dependents_deletion.is_empty() {
            // All owners are solid, no action needed
            return Ok(());
        }

        // Remove dangling and waiting_for_dependents_deletion owners from owner references
        // waiting_for_dependents_deletion owners need to be deleted from the
        // ownerReferences, otherwise the referenced objects will be stuck with
        // the DeletingDependents finalizer and never get deleted.
        let new_owner_refs: Vec<OwnerReference> = solid.clone();
        patch_new_owner_references(
            xline_store.as_ref(),
            node.as_ref(),
            &obj_yaml,
            new_owner_refs,
        )
        .await?;
    } else if !waiting_for_dependents_deletion.is_empty() && dependents_length > 0 {
        for dependent in dependents_snapshot {
            let (owner_refs, dependent_identity, updated) = {
                let dep_guard = dependent.read().await;
                let mut owner_refs = dep_guard.owners().clone();
                let mut changed = false;
                for owner_ref in owner_refs.iter_mut() {
                    if owner_ref.uid == identity.uid && owner_ref.block_owner_deletion == Some(true)
                    {
                        owner_ref.block_owner_deletion = Some(false);
                        changed = true;
                    }
                }
                (owner_refs, dep_guard.identity().clone(), changed)
            };

            if !updated {
                continue;
            }

            if let Some(dependent_yaml) =
                get_object_yaml(xline_store.as_ref(), &dependent_identity).await?
            {
                patch_new_owner_references(
                    xline_store.as_ref(),
                    dependent.as_ref(),
                    &dependent_yaml,
                    owner_refs,
                )
                .await?;
            }
        }

        log::debug!(
            "[Garbage Collector] At least one owner of item has DeleteDependentsFinalizer, and the item itself has dependents, so it is going to be deleted in Foreground"
        );

        delete_object(
            xline_store.as_ref(),
            &identity,
            DeletePropagationPolicy::Foreground,
        )
        .await?;
    } else {
        let policy = if has_orphan_dependents_finalizer(&meta) {
            DeletePropagationPolicy::Orphan
        } else if has_delete_dependents_finalizer(&meta) {
            DeletePropagationPolicy::Foreground
        } else {
            DeletePropagationPolicy::Background
        };

        log::debug!(
            "Deleting object {} of kind {:?} with policy {:?}",
            identity.name,
            identity.kind,
            policy
        );

        delete_object(xline_store.as_ref(), &identity, policy).await?;
    }

    Ok(())
}

async fn delete_object(
    xline_store: &XlineStore,
    identity: &ObjectReference,
    policy: DeletePropagationPolicy,
) -> anyhow::Result<()> {
    xline_store
        .delete_object(identity.kind, &identity.name, policy)
        .await
}

async fn get_object_yaml(
    xline_store: &XlineStore,
    identity: &ObjectReference,
) -> anyhow::Result<Option<String>> {
    let yaml = xline_store
        .get_object_yaml(identity.kind, &identity.name)
        .await?;
    Ok(yaml)
}

async fn patch_new_owner_references(
    xline_store: &XlineStore,
    node: &RwLock<Node>,
    origin_yaml: &str,
    new_owner_refs: Vec<OwnerReference>,
) -> anyhow::Result<()> {
    // patch the object yaml to update owner references
    let mut meta = parse_meta_from_yaml(origin_yaml)?;

    if new_owner_refs.is_empty() {
        meta.owner_references = None;
    } else {
        meta.owner_references = Some(new_owner_refs.clone());
    }
    let mut yaml_value: serde_yaml::Value = serde_yaml::from_str(origin_yaml)?;
    let updated_meta_yaml_value = serde_yaml::to_value(meta)?;
    let meta_map_mut = yaml_value
        .as_mapping_mut()
        .and_then(|m| m.get_mut(serde_yaml::Value::String("metadata".to_string())))
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| anyhow::anyhow!("Failed to get mutable metadata map"))?;
    *meta_map_mut = updated_meta_yaml_value
        .as_mapping()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert updated meta to mapping"))?;
    let updated_yaml = serde_yaml::to_string(&yaml_value)?;

    let read_guard = node.read().await;
    xline_store
        .insert_object_yaml(
            read_guard.identity().kind,
            &read_guard.identity().name,
            &updated_yaml,
        )
        .await?;
    Ok(())
}

// Check if an owner exists in xline
async fn is_dangling_owner(
    owner_ref: &OwnerReference,
    node: &RwLock<Node>,
    xline_store: &XlineStore,
    absent_owner_cache: Arc<RwLock<LruCache<ObjectReference, ()>>>,
) -> anyhow::Result<(bool, Option<ObjectMeta>)> {
    if absent_owner_cache
        .read()
        .await
        .contains(&ObjectReference::from_owner_reference(
            owner_ref,
            node.read().await.identity().namespace.clone(),
        ))
    {
        return Ok((true, None));
    }

    let owner_yaml = get_object_yaml(
        xline_store,
        &ObjectReference::new(
            owner_ref.kind,
            owner_ref.name.clone(),
            owner_ref.uid,
            node.read().await.identity().namespace.clone(),
            None,
        ),
    )
    .await?;

    if let Some(yaml) = owner_yaml {
        let meta = parse_meta_from_yaml(&yaml)?;
        if meta.uid != owner_ref.uid {
            // Owner UID mismatch, consider dangling
            absent_owner_cache.write().await.put(
                ObjectReference::from_owner_reference(
                    owner_ref,
                    node.read().await.identity().namespace.clone(),
                ),
                (),
            );
            return Ok((true, None));
        }
        Ok((false, Some(meta)))
    } else {
        // Owner does not exist, consider dangling
        absent_owner_cache.write().await.put(
            ObjectReference::from_owner_reference(
                owner_ref,
                node.read().await.identity().namespace.clone(),
            ),
            (),
        );
        Ok((true, None))
    }
}

/// Classify owner references into:
/// - Solid owners: owner exists and is not waiting for dependents deletion
/// - Dangling owners: owner does not exist
/// - Waiting for dependents deletion: owner exists and its deletion timestamp is set and has deleting dependents finalizer
///
/// Returns three vectors of OwnerReference (solid, dangling, waiting_for_dependents_deletion)
async fn classify_owners(
    owner_refs: &[OwnerReference],
    node: &RwLock<Node>,
    xline_store: &XlineStore,
    absent_owner_cache: Arc<RwLock<LruCache<ObjectReference, ()>>>,
) -> anyhow::Result<(
    Vec<OwnerReference>,
    Vec<OwnerReference>,
    Vec<OwnerReference>,
)> {
    let mut solid = Vec::new();
    let mut dangling = Vec::new();
    let mut waiting_for_dependents_deletion = Vec::new();

    for owner_ref in owner_refs {
        let (is_dangling, owner_meta_opt) =
            is_dangling_owner(owner_ref, node, xline_store, absent_owner_cache.clone()).await?;

        if is_dangling {
            dangling.push(owner_ref.clone());
        } else if let Some(owner_meta) = owner_meta_opt {
            if owner_meta.deletion_timestamp.is_some()
                && has_delete_dependents_finalizer(&owner_meta)
            {
                waiting_for_dependents_deletion.push(owner_ref.clone());
            } else {
                solid.push(owner_ref.clone());
            }
        }
    }

    Ok((solid, dangling, waiting_for_dependents_deletion))
}

/// Process node which is waiting for its dependents to be deleted
async fn process_deleting_dependents(
    xline_store: Arc<XlineStore>,
    attempt_to_delete_tx: UnboundedSender<Arc<RwLock<Node>>>,
    node: Arc<RwLock<Node>>,
) -> Result<(), AttemptToDeleteItemError> {
    let blocking_dependents = node.read().await.blocking_dependents().await;
    if blocking_dependents.is_empty() {
        log::debug!(
            "[Garbage Collector] No more blocking dependents, removing DeletingDependents finalizer from {}",
            node.read().await.identity().name
        );

        // Remove DeletingDependents finalizer
        remove_finalizer(&xline_store, &node, Finalizer::DeletingDependents).await?;
    } else {
        for dependent in blocking_dependents {
            if !dependent.read().await.is_deleting_dependents() {
                log::debug!(
                    "[Garbage Collector] Found dependent {} that not in deleting dependents state yet, sending attempt to delete because its owner is waiting for dependents deletion",
                    dependent.read().await.identity().name
                );

                // Send attempt to delete for the dependent
                let _ = attempt_to_delete_tx.send(dependent.clone());
            }
        }
    }

    Ok(())
}

async fn remove_finalizer(
    xline_store: &XlineStore,
    node: &RwLock<Node>,
    finalizer: Finalizer,
) -> anyhow::Result<()> {
    let read_guard = node.read().await;
    let origin_yaml = xline_store
        .get_object_yaml(read_guard.identity().kind, &read_guard.identity().name)
        .await?;

    if origin_yaml.is_none() {
        return Ok(());
    }

    let origin_yaml = origin_yaml.unwrap();
    let mut meta = parse_meta_from_yaml(&origin_yaml)?;

    if let Some(finalizers) = &mut meta.finalizers {
        finalizers.retain(|f| f != &finalizer);
    }
    let mut yaml_value: serde_yaml::Value = serde_yaml::from_str(&origin_yaml)?;
    let updated_meta_yaml_value = serde_yaml::to_value(meta)?;
    let meta_map_mut = yaml_value
        .as_mapping_mut()
        .and_then(|m| m.get_mut(serde_yaml::Value::String("metadata".to_string())))
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| anyhow::anyhow!("Failed to get mutable metadata map"))?;
    *meta_map_mut = updated_meta_yaml_value
        .as_mapping()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert updated meta to mapping"))?;
    let updated_yaml = serde_yaml::to_string(&yaml_value)?;

    xline_store
        .insert_object_yaml(
            read_guard.identity().kind,
            &read_guard.identity().name,
            &updated_yaml,
        )
        .await?;

    Ok(())
}

pub fn parse_meta_from_yaml(yaml: &str) -> anyhow::Result<ObjectMeta> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let meta_value = &doc["metadata"];
    let meta = serde_yaml::from_value::<ObjectMeta>(meta_value.clone())?;
    Ok(meta)
}
