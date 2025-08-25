//! S3-compatible backend adapter (placeholder)
//!
//! Minimal S3-compatible adapter scaffold. Replace with an implementation using
//! `aws-sdk-s3`, `reqwest`, or another HTTP client as desired.

#[allow(dead_code)]
pub async fn put(_key: &str, _data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: implement S3 upload
    Ok(())
}

#[allow(dead_code)]
pub async fn get(_key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    // TODO: implement S3 download
    Ok(None)
}
