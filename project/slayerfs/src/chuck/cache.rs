use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CacheItem {
    pub file_path: PathBuf,
    pub etag: String,
}
