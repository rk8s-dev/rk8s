use serial_test::serial;

use anyhow::Result;
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use rkforge::config::meta::Repositories;
use rkforge::images::{
    ImagesArgs, InspectArgs, LoadArgs, RmiArgs, SaveArgs, TagArgs, inspect_image, list_images,
    load_image, remove_image, save_image, tag_image,
};
use rkforge::storage::ultimate_blob_path;
use sha2::{Digest, Sha256};
use std::fs;
use std::sync::OnceLock;

/// Redirect rkforge storage to a per-process temp directory so tests never
/// touch the real blob cache or `repositories.toml`.  The `OnceLock` ensures
/// the env var is set exactly once, before `CONFIG` is lazily initialized.
static TEST_STORAGE: OnceLock<tempfile::TempDir> = OnceLock::new();

fn ensure_test_storage() {
    TEST_STORAGE.get_or_init(|| {
        let dir = tempfile::Builder::new()
            .prefix("rkforge-test-")
            .tempdir()
            .expect("Failed to create test storage root");
        std::env::set_var("RKFORGE_STORAGE_ROOT", dir.path());
        dir
    });
}

/// Write a blob (bytes) into local storage under the given digest and return
/// the digest string.  The parent directory is created if absent.
fn write_blob(digest: &str, content: &[u8]) -> Result<()> {
    let path = ultimate_blob_path(digest)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Provision a minimal fake OCI image in local storage and register it under
/// the given `image_ref`.  Returns the manifest digest string.
fn provision_test_image(image_ref: &str) -> Result<String> {
    let config_json = serde_json::json!({
        "created": "2025-01-01T00:00:00Z",
        "architecture": "amd64",
        "os": "linux",
        "rootfs": {
            "type": "layers",
            "diff_ids": ["sha256:0000000000000000000000000000000000000000000000000000000000000000"]
        }
    });
    let config_bytes = serde_json::to_vec_pretty(&config_json)?;
    let config_hash = sha256_hex(&config_bytes);
    let config_digest = format!("sha256:{config_hash}");
    write_blob(&config_digest, &config_bytes)?;

    // Create a minimal layer directory (layers are stored as unpacked dirs)
    let layer_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let layer_digest = format!("sha256:{layer_hash}");
    let layer_path = ultimate_blob_path(&layer_digest)?;
    fs::create_dir_all(&layer_path)?;
    fs::write(layer_path.join("hello.txt"), "test layer content")?;

    let manifest = OciImageManifest {
        schema_version: 2,
        media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
        config: OciDescriptor {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            digest: config_digest,
            size: config_bytes.len() as i64,
            urls: None,
            annotations: None,
        },
        layers: vec![OciDescriptor {
            media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
            digest: layer_digest,
            size: 0,
            urls: None,
            annotations: None,
        }],
        subject: None,
        artifact_type: None,
        annotations: None,
    };

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_hash = sha256_hex(&manifest_bytes);
    let manifest_digest = format!("sha256:{manifest_hash}");
    write_blob(&manifest_digest, &manifest_bytes)?;

    Repositories::add_store(vec![(image_ref.to_string(), manifest_digest.clone())])?;
    Ok(manifest_digest)
}

/// Remove a test image and its blobs from local storage (best-effort).
fn cleanup_test_image(image_ref: &str) {
    let _ = remove_image(RmiArgs {
        image_ref: image_ref.to_string(),
        force: true,
    });
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[test]
#[serial]
fn test_list_images() {
    ensure_test_storage();
    let image_ref = "library/test-list:latest";
    let _ = provision_test_image(image_ref);

    let result = list_images(ImagesArgs { quiet: false });
    assert!(result.is_ok(), "list_images should succeed: {result:?}");

    let result_quiet = list_images(ImagesArgs { quiet: true });
    assert!(
        result_quiet.is_ok(),
        "list_images quiet should succeed: {result_quiet:?}"
    );

    cleanup_test_image(image_ref);
}

#[test]
#[serial]
fn test_inspect_image() {
    ensure_test_storage();
    let image_ref = "library/test-inspect:latest";
    let _ = provision_test_image(image_ref);

    let result = inspect_image(InspectArgs {
        image_ref: image_ref.to_string(),
    });
    assert!(result.is_ok(), "inspect_image should succeed: {result:?}");

    cleanup_test_image(image_ref);
}

#[test]
#[serial]
fn test_inspect_not_found() {
    ensure_test_storage();
    let result = inspect_image(InspectArgs {
        image_ref: "library/nonexistent:latest".to_string(),
    });
    assert!(
        result.is_err(),
        "inspect_image should fail for missing image"
    );
}

#[test]
#[serial]
fn test_tag_image() {
    ensure_test_storage();
    let source = "library/test-tag-src:latest";
    let target = "library/test-tag-dst:v1";
    let _ = provision_test_image(source);

    let result = tag_image(TagArgs {
        source: source.to_string(),
        target: target.to_string(),
    });
    assert!(result.is_ok(), "tag_image should succeed: {result:?}");

    let repos = Repositories::load().unwrap();
    assert!(
        repos.get(target).unwrap().is_some(),
        "target tag should exist in repositories"
    );

    cleanup_test_image(source);
    cleanup_test_image(target);
}

#[test]
#[serial]
fn test_rmi_basic() {
    ensure_test_storage();
    let image_ref = "library/test-rmi:latest";
    let _ = provision_test_image(image_ref);

    let result = remove_image(RmiArgs {
        image_ref: image_ref.to_string(),
        force: false,
    });
    assert!(result.is_ok(), "remove_image should succeed: {result:?}");

    let repos = Repositories::load().unwrap();
    assert!(
        repos.get(image_ref).unwrap().is_none(),
        "image should be removed from repositories"
    );
}

#[test]
#[serial]
fn test_rmi_reference_counting() {
    ensure_test_storage();
    let tag_a = "library/test-refcount-a:latest";
    let tag_b = "library/test-refcount-b:latest";
    let digest = provision_test_image(tag_a).unwrap();

    // Create a second tag pointing to the same manifest
    tag_image(TagArgs {
        source: tag_a.to_string(),
        target: tag_b.to_string(),
    })
    .unwrap();

    // Remove first tag — blobs should still exist
    remove_image(RmiArgs {
        image_ref: tag_a.to_string(),
        force: false,
    })
    .unwrap();

    let manifest_path = ultimate_blob_path(&digest).unwrap();
    assert!(
        manifest_path.exists(),
        "manifest blob should still exist while tag_b references it"
    );

    // Remove second tag — blobs should be cleaned up
    remove_image(RmiArgs {
        image_ref: tag_b.to_string(),
        force: false,
    })
    .unwrap();

    assert!(
        !manifest_path.exists(),
        "manifest blob should be removed after all tags are gone"
    );
}

#[test]
#[serial]
fn test_rmi_not_found() {
    ensure_test_storage();
    let result = remove_image(RmiArgs {
        image_ref: "library/nonexistent:latest".to_string(),
        force: false,
    });
    assert!(
        result.is_err(),
        "remove_image should fail for missing image"
    );
}

#[test]
#[serial]
fn test_save_and_load_roundtrip() {
    ensure_test_storage();
    let original_ref = "library/test-save:latest";
    let _ = provision_test_image(original_ref);

    let tmp_dir = tempfile::tempdir().unwrap();
    let archive_path = tmp_dir.path().join("test-image.tar");

    // Save
    let save_result = save_image(SaveArgs {
        image_ref: original_ref.to_string(),
        output: archive_path.to_str().unwrap().to_string(),
    });
    assert!(
        save_result.is_ok(),
        "save_image should succeed: {save_result:?}"
    );
    assert!(archive_path.exists(), "tar archive should be created");

    // Load under a different tag
    let loaded_ref = "library/test-loaded:v1";
    let load_result = load_image(LoadArgs {
        input: archive_path.to_str().unwrap().to_string(),
        tag: Some(loaded_ref.to_string()),
    });
    assert!(
        load_result.is_ok(),
        "load_image should succeed: {load_result:?}"
    );

    let repos = Repositories::load().unwrap();
    assert!(
        repos.get("library/test-loaded:v1").unwrap().is_some(),
        "loaded image should exist in repositories"
    );

    cleanup_test_image(original_ref);
    cleanup_test_image("library/test-loaded:v1");
}

#[test]
#[serial]
fn test_save_output_already_exists() {
    ensure_test_storage();
    let image_ref = "library/test-save-dup:latest";
    let _ = provision_test_image(image_ref);

    let tmp_dir = tempfile::tempdir().unwrap();
    let archive_path = tmp_dir.path().join("existing.tar");
    fs::write(&archive_path, "placeholder").unwrap();

    let result = save_image(SaveArgs {
        image_ref: image_ref.to_string(),
        output: archive_path.to_str().unwrap().to_string(),
    });
    assert!(
        result.is_err(),
        "save_image should fail when output file exists"
    );

    cleanup_test_image(image_ref);
}

#[test]
#[serial]
fn test_load_nonexistent_file() {
    ensure_test_storage();
    let result = load_image(LoadArgs {
        input: "/tmp/nonexistent-image-archive.tar".to_string(),
        tag: Some("library/test:latest".to_string()),
    });
    assert!(
        result.is_err(),
        "load_image should fail for missing input file"
    );
}

#[test]
#[serial]
fn test_load_invalid_tar() {
    ensure_test_storage();
    let tmp_dir = tempfile::tempdir().unwrap();
    let bad_tar = tmp_dir.path().join("bad.tar");
    fs::write(&bad_tar, "this is not a tar file").unwrap();

    let result = load_image(LoadArgs {
        input: bad_tar.to_str().unwrap().to_string(),
        tag: Some("library/test-bad:latest".to_string()),
    });
    assert!(
        result.is_err(),
        "load_image should fail for invalid tar archive"
    );
}
