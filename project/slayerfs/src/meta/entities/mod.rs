// Core entities for decoupled metadata model
pub mod access_meta;
pub mod content_meta;
pub mod etcd;
pub mod file_meta;
pub mod link_parent_meta;
pub mod locks_meta;
pub mod plock_meta;
pub mod session_meta;
pub mod slice_meta;
pub mod xattr_meta;

pub(crate) use access_meta::{Entity as AccessMeta, Model as AccessMetaModel};
pub(crate) use content_meta::{Entity as ContentMeta, EntryType, Model as ContentMetaModel};
pub(crate) use file_meta::{Entity as FileMeta, Model as FileMetaModel};
pub(crate) use link_parent_meta::Entity as LinkParentMeta;
pub(crate) use locks_meta::Entity as LocksMeta;
pub(crate) use plock_meta::Entity as PlockMeta;
#[allow(unused_imports)]
pub use slice_meta::{Entity as SliceMeta, Model as SliceMetaModel};
pub use xattr_meta::Entity as XattrMeta;
