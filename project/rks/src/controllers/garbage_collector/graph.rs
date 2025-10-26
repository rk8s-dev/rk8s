use std::{collections::HashMap, sync::Arc};

use common::{OwnerReference, PodTask, ResourceKind};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::types::ObjectReference;

#[derive(Debug, Clone)]
pub struct Node {
    identity: ObjectReference,
    dependents: Vec<Arc<RwLock<Node>>>,
    owners: Vec<OwnerReference>,
    is_virtual: bool,
    deleting_dependents: bool,
    being_deleted: bool,
}

impl Node {
    pub fn new(identity: ObjectReference, is_virtual: bool) -> Self {
        Self {
            identity,
            dependents: Vec::new(),
            owners: Vec::new(),
            is_virtual,
            deleting_dependents: false,
            being_deleted: false,
        }
    }

    pub fn is_virtual(&self) -> bool {
        self.is_virtual
    }

    pub fn is_observed(&self) -> bool {
        !self.is_virtual
    }

    pub fn set_observed(&mut self) {
        self.is_virtual = false;
    }

    pub fn is_being_deleted(&self) -> bool {
        self.being_deleted
    }

    pub fn set_being_deleted(&mut self) {
        self.being_deleted = true;
    }

    pub fn is_deleting_dependents(&self) -> bool {
        self.deleting_dependents
    }

    pub fn set_deleting_dependents(&mut self) {
        self.deleting_dependents = true;
    }

    pub fn add_dependent(&mut self, node: Arc<RwLock<Node>>) {
        self.dependents.push(node);
    }

    pub fn remove_dependent(&mut self, node: Arc<RwLock<Node>>) {
        self.dependents.retain(|n| !Arc::ptr_eq(n, &node));
    }

    pub fn dependents(&self) -> &Vec<Arc<RwLock<Node>>> {
        &self.dependents
    }

    pub async fn blocking_dependents(&self) -> Vec<Arc<RwLock<Node>>> {
        let mut result = Vec::new();
        for n in &self.dependents {
            let n_guard = n.read().await;
            for owner in &n_guard.owners {
                if owner.block_owner_deletion.unwrap_or(false) && owner.uid == self.identity.uid {
                    result.push(n.clone());
                }
            }
        }
        result
    }

    pub fn dependents_length(&self) -> usize {
        self.dependents.len()
    }

    pub fn identity(&self) -> &ObjectReference {
        &self.identity
    }

    pub fn set_identity(&mut self, identity: ObjectReference) {
        self.identity = identity;
    }

    pub fn owners_mut(&mut self) -> &mut Vec<OwnerReference> {
        &mut self.owners
    }

    pub fn owners(&self) -> &Vec<OwnerReference> {
        &self.owners
    }

    pub fn set_owners(&mut self, owners: Vec<OwnerReference>) {
        self.owners = owners;
    }
}

impl From<PodTask> for Node {
    fn from(pod_task: PodTask) -> Self {
        let identity = ObjectReference::new(
            ResourceKind::Pod,
            pod_task.metadata.name,
            pod_task.metadata.uid,
            pod_task.metadata.namespace,
            pod_task
                .metadata
                .owner_references
                .and_then(|ors| ors.iter().find_map(|owner| owner.block_owner_deletion)),
        );
        Self::new(identity, false)
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for Node {}

#[derive(Debug, Default)]
pub struct UidToNodeTable {
    table: RwLock<HashMap<Uuid, Arc<RwLock<Node>>>>,
}

impl UidToNodeTable {
    pub async fn insert(&self, uid: Uuid, node: Arc<RwLock<Node>>) {
        let mut table = self.table.write().await;
        table.insert(uid, node);
    }

    pub async fn get(&self, uid: &Uuid) -> Option<Arc<RwLock<Node>>> {
        let table = self.table.read().await;
        table.get(uid).cloned()
    }

    pub async fn remove(&self, uid: &Uuid) -> Option<Arc<RwLock<Node>>> {
        let mut table = self.table.write().await;
        table.remove(uid)
    }

    #[allow(dead_code)]
    pub async fn contains_key(&self, uid: &Uuid) -> bool {
        let table = self.table.read().await;
        table.contains_key(uid)
    }
}
