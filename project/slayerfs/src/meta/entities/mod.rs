// Core entities for decoupled metadata model
pub mod access_meta;
pub mod content_meta;
pub mod file_meta;
pub mod xline;
pub use access_meta::{Entity as AccessMeta, Model as AccessMetaModel};
pub use content_meta::{Entity as ContentMeta, EntryType, Model as ContentMetaModel};
pub use file_meta::{Entity as FileMeta, Model as FileMetaModel};
