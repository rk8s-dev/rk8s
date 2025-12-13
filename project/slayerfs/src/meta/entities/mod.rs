// Core entities for decoupled metadata model
pub mod access_meta;
pub mod content_meta;
pub mod etcd;
pub mod file_meta;
pub mod locks_meta;
pub mod session_meta;
pub mod slice_meta;

pub use access_meta::{Entity as AccessMeta, Model as AccessMetaModel};
pub use content_meta::{Entity as ContentMeta, EntryType, Model as ContentMetaModel};
pub use file_meta::{Entity as FileMeta, Model as FileMetaModel};
pub use locks_meta::Entity as LocksMeta;
#[allow(unused_imports)]
pub use slice_meta::{Entity as SliceMeta, Model as SliceMetaModel};
