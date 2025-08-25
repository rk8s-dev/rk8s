//! High-level client API for chunk/object store
//!
//! This module exports the main client used by writer/reader code to PUT/GET
//! block objects. Start with simple async functions that wrap the lower-level
//! backends.

#[allow(dead_code)]
pub async fn put_object(_key: &str, _data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: implement
    Ok(())
}

#[allow(dead_code)]
pub async fn get_object(_key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    // TODO: implement
    Ok(None)
}
