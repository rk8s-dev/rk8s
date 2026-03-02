use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// Build result metadata for `--metadata-file` output.
#[derive(Debug, Serialize)]
pub struct BuildMetadata {
    pub tags: Vec<String>,
    pub digest: String,
    pub id: String,
    pub build_args: HashMap<String, String>,
    pub duration_ms: u128,
}

/// Write build metadata to a JSON file.
///
/// Creates parent directories if they don't exist.
pub fn write_metadata_file<P: AsRef<Path>>(path: P, metadata: &BuildMetadata) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory {}", parent.display()))?;
    }

    let payload =
        serde_json::to_vec_pretty(metadata).context("Failed to serialize build metadata")?;
    fs::write(path, payload).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_metadata_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("meta").join("build.json");
        let metadata = BuildMetadata {
            tags: vec!["repo/app:latest".to_string()],
            digest: "sha256:abc".to_string(),
            id: "sha256:abc".to_string(),
            build_args: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            duration_ms: 42,
        };

        write_metadata_file(&output, &metadata).unwrap();
        let content = fs::read_to_string(output).unwrap();
        assert!(content.contains("\"tags\""));
        assert!(content.contains("\"duration_ms\": 42"));
    }
}
