//! Metadata SQL client (placeholder)
//!
//! Provide transactional helpers wrapping SQLx operations for inodes, chunks,
//! slices and blocks.

#[allow(dead_code)]
pub async fn connect(_dsn: &str) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: implement connection and migrations
    Ok(())
}
