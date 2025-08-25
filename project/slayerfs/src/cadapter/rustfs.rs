//! rustfs backend adapter (placeholder)
//!
//! Implement backend-specific logic to talk to rustfs. This file should expose
//! the same signatures expected by `client` but implement them against rustfs.

#[allow(dead_code)]
pub async fn put(_key: &str, _data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: implement rustfs upload
    Ok(())
}

#[allow(dead_code)]
pub async fn get(_key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    // TODO: implement rustfs download
    Ok(None)
}
