use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Calculates the SHA256 digest for a given file path and returns it as a hex string.
pub fn calculate_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    let file = File::open(path.as_ref()).with_context(|| {
        format!(
            "Failed to open file for hashing: {}",
            path.as_ref().display()
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    io::copy(&mut reader, &mut hasher)?;
    let hash_bytes = hasher.finalize();
    Ok(format!("{:x}", hash_bytes))
}
