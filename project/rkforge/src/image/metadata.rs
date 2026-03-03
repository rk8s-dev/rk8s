use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub(super) struct BuildMetadata {
    tags: Vec<String>,
    digest: String,
    id: String,
    build_args: HashMap<String, String>,
    duration_ms: u128,
}

impl BuildMetadata {
    pub(super) fn new(
        tags: Vec<String>,
        digest: String,
        id: String,
        build_args: HashMap<String, String>,
        duration_ms: u128,
    ) -> Self {
        Self {
            tags,
            digest,
            id,
            build_args,
            duration_ms,
        }
    }
}

pub(super) fn write_metadata_file<P: AsRef<Path>>(path: P, metadata: &BuildMetadata) -> Result<()> {
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
    use std::collections::HashMap;
    use std::fs;

    use super::{BuildMetadata, write_metadata_file};

    #[test]
    fn test_write_metadata_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("meta").join("build.json");
        let metadata = BuildMetadata::new(
            vec!["repo/app:latest".to_string()],
            "sha256:abc".to_string(),
            "sha256:abc".to_string(),
            HashMap::from([("FOO".to_string(), "bar".to_string())]),
            42,
        );

        write_metadata_file(&output, &metadata).unwrap();
        let content = fs::read_to_string(output).unwrap();
        assert!(content.contains("\"tags\""));
        assert!(content.contains("\"duration_ms\": 42"));
    }
}
