use common::{ObjectMeta, OwnerReference, ResourceKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectReference {
    pub kind: ResourceKind,
    pub name: String,
    pub uid: Uuid,
    pub namespace: String,
    #[serde(default)]
    pub block_owner_deletion: Option<bool>,
}

impl ObjectReference {
    pub fn new(
        kind: ResourceKind,
        name: String,
        uid: Uuid,
        namespace: String,
        block_owner_deletion: Option<bool>,
    ) -> Self {
        Self {
            kind,
            name,
            uid,
            namespace,
            block_owner_deletion,
        }
    }

    pub fn from_meta(meta: &ObjectMeta, kind: ResourceKind) -> Self {
        Self {
            kind,
            name: meta.name.clone(),
            uid: meta.uid,
            namespace: meta.namespace.clone(),
            block_owner_deletion: meta.owner_references.as_ref().and_then(|owners| {
                owners.iter().find_map(|owner| {
                    if owner.controller {
                        owner.block_owner_deletion
                    } else {
                        None
                    }
                })
            }),
        }
    }

    pub fn from_owner_reference(or: &OwnerReference, namespace: String) -> Self {
        Self {
            kind: or.kind,
            name: or.name.clone(),
            uid: or.uid,
            namespace,
            block_owner_deletion: or.block_owner_deletion,
        }
    }
}
