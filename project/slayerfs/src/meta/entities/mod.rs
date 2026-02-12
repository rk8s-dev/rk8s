// Core entities for decoupled metadata model
pub(crate) mod access_meta;
pub(crate) mod content_meta;
pub(crate) mod counter_meta;
pub(crate) mod etcd;
pub(crate) mod file_meta;
pub(crate) mod link_parent_meta;
pub(crate) mod locks_meta;
pub(crate) mod plock_meta;
pub(crate) mod session_meta;
pub(crate) mod slice_meta;
pub(crate) mod xattr_meta;

pub(crate) use access_meta::{Entity as AccessMeta, Model as AccessMetaModel};
pub(crate) use content_meta::{Entity as ContentMeta, EntryType, Model as ContentMetaModel};
pub(crate) use counter_meta::Entity as CounterMeta;
pub(crate) use file_meta::{Entity as FileMeta, Model as FileMetaModel};
pub(crate) use link_parent_meta::Entity as LinkParentMeta;
pub(crate) use locks_meta::Entity as LocksMeta;
pub(crate) use plock_meta::Entity as PlockMeta;
#[allow(unused_imports)]
pub(crate) use slice_meta::{Entity as SliceMeta, Model as SliceMetaModel};
pub(crate) use xattr_meta::Entity as XattrMeta;
