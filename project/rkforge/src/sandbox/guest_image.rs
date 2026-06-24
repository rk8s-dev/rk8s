use crate::pull;
use crate::rt;
use crate::sandbox::runtime_assets::RuntimeAssetBundle;
use anyhow::{Context, Result, anyhow, bail};
use libruntime::bundle;
use libruntime::utils::{ImageType, determine_image};
use serde::{Deserialize, Serialize};
use sha256::try_digest;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

const GUEST_IMAGE_OVERRIDE_ENV: &str = "RKFORGE_SANDBOX_GUEST_IMAGE";
const GUEST_IMAGE_CACHE_DIR: &str = "guest-images";
const GUEST_IMAGE_TMP_DIR: &str = "tmp";
const GUEST_IMAGE_FILENAME: &str = "guest-rootfs.ext4";
const GUEST_IMAGE_METADATA_FILE: &str = "guest-image.json";
const GUEST_IMAGE_SIZE_PADDING_MIB: u64 = 256;
const GUEST_RKFORGE_PATH: &str = "usr/local/bin/rkforge";
const GUEST_AGENT_ALIAS_PATH: &str = "usr/local/bin/rkforge-sandbox-agent";
const GUEST_INIT_ALIAS_PATH: &str = "usr/local/bin/rkforge-sandbox-guest-init";

#[derive(Debug, Clone)]
pub struct GuestImageManager {
    root: PathBuf,
    guest_binary: PathBuf,
    agent_binary: PathBuf,
    guest_init_binary: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct GuestImageMetadata {
    source_image: String,
    manifest_digest: String,
    guest_binary: PathBuf,
    guest_binary_sha256: String,
    guest_image: PathBuf,
}

impl GuestImageManager {
    pub fn new(root: PathBuf) -> Result<Self> {
        let assets = RuntimeAssetBundle::prepare(&root)?;
        let guest_binary = assets.rkforge_binary().to_path_buf();
        let agent_binary = assets.agent_binary().to_path_buf();
        let guest_init_binary = assets.guest_init_binary().to_path_buf();
        if !guest_binary.is_file() {
            bail!(
                "sandbox guest binary does not exist: {}",
                guest_binary.display()
            );
        }
        Ok(Self {
            root,
            guest_binary,
            agent_binary,
            guest_init_binary,
        })
    }

    pub fn ensure_guest_image(&self, image_ref: &str) -> Result<PathBuf> {
        if let Some(path) = std::env::var_os(GUEST_IMAGE_OVERRIDE_ENV).map(PathBuf::from) {
            debug!(
                path=%path.display(),
                "using explicit sandbox guest image override"
            );
            return Ok(path);
        }

        match determine_image(image_ref)? {
            ImageType::Bundle => bail!(
                "automatic sandbox guest image building currently supports OCI image refs only; got local bundle path `{image_ref}`"
            ),
            ImageType::OCIImage => {}
        }

        ensure_tool("mkfs.ext4")?;

        let (manifest_path, layers) = pull::sync_pull_or_get_image(image_ref, None::<&str>)
            .with_context(|| format!("failed to pull sandbox guest image `{image_ref}`"))?;
        let manifest_digest = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow!(
                    "failed to derive manifest digest from {}",
                    manifest_path.display()
                )
            })?;
        let guest_binary_sha256 = try_digest(&self.guest_binary).with_context(|| {
            format!(
                "failed to hash sandbox guest binary {}",
                self.guest_binary.display()
            )
        })?;
        let cache_key = cache_key(&manifest_digest, &guest_binary_sha256);
        let cache_dir = self.root.join(GUEST_IMAGE_CACHE_DIR).join(cache_key);
        let guest_image = cache_dir.join(GUEST_IMAGE_FILENAME);
        if guest_image.is_file() {
            debug!(
                source_image=%image_ref,
                guest_image=%guest_image.display(),
                "reusing cached sandbox guest image"
            );
            return Ok(guest_image);
        }

        fs::create_dir_all(self.root.join(GUEST_IMAGE_CACHE_DIR)).with_context(|| {
            format!(
                "failed to create sandbox guest image cache root {}",
                self.root.join(GUEST_IMAGE_CACHE_DIR).display()
            )
        })?;
        fs::create_dir_all(self.root.join(GUEST_IMAGE_TMP_DIR)).with_context(|| {
            format!(
                "failed to create sandbox guest image tmp root {}",
                self.root.join(GUEST_IMAGE_TMP_DIR).display()
            )
        })?;

        info!(
            source_image=%image_ref,
            manifest_digest=%manifest_digest,
            guest_binary=%self.guest_binary.display(),
            "building sandbox guest image"
        );
        let build_dir = Builder::new()
            .prefix("guest-image-")
            .tempdir_in(self.root.join(GUEST_IMAGE_TMP_DIR))
            .context("failed to create temporary sandbox guest image build directory")?;
        let bundle_dir = build_dir.path().join("bundle");
        rt::block_on(bundle::mount_and_copy_bundle(&bundle_dir, &layers))??;

        let rootfs_dir = bundle_dir.join("rootfs");
        prepare_guest_rootfs(
            &rootfs_dir,
            &self.guest_binary,
            &self.agent_binary,
            &self.guest_init_binary,
        )?;

        let built_image = build_dir.path().join(GUEST_IMAGE_FILENAME);
        build_ext4_image(&rootfs_dir, &built_image)?;

        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create {}", cache_dir.display()))?;
        if guest_image.is_file() {
            return Ok(guest_image);
        }
        fs::rename(&built_image, &guest_image).with_context(|| {
            format!(
                "failed to move sandbox guest image into cache {}",
                guest_image.display()
            )
        })?;

        let metadata = GuestImageMetadata {
            source_image: image_ref.to_string(),
            manifest_digest,
            guest_binary: self.guest_binary.clone(),
            guest_binary_sha256,
            guest_image: guest_image.clone(),
        };
        fs::write(
            cache_dir.join(GUEST_IMAGE_METADATA_FILE),
            serde_json::to_vec_pretty(&metadata)?,
        )
        .with_context(|| {
            format!(
                "failed to write sandbox guest image metadata into {}",
                cache_dir.display()
            )
        })?;

        Ok(guest_image)
    }
}

fn cache_key(manifest_digest: &str, guest_binary_sha256: &str) -> String {
    format!(
        "{}-{}",
        shorten_digest(manifest_digest),
        shorten_digest(guest_binary_sha256)
    )
}

fn shorten_digest(value: &str) -> &str {
    value.get(..16).unwrap_or(value)
}

fn ensure_tool(binary: &str) -> Result<()> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if dir.join(binary).is_file() {
            return Ok(());
        }
    }
    bail!("`{binary}` is required to build sandbox guest images automatically")
}

fn prepare_guest_rootfs(
    rootfs_dir: &Path,
    guest_binary: &Path,
    agent_binary: &Path,
    guest_init_binary: &Path,
) -> Result<()> {
    fs::create_dir_all(rootfs_dir.join("usr/local/bin")).with_context(|| {
        format!(
            "failed to create {}",
            rootfs_dir.join("usr/local/bin").display()
        )
    })?;
    for dir in ["proc", "sys", "dev", "run", "tmp"] {
        fs::create_dir_all(rootfs_dir.join(dir))
            .with_context(|| format!("failed to create {}", rootfs_dir.join(dir).display()))?;
    }

    let installed_guest_binary = rootfs_dir.join(GUEST_RKFORGE_PATH);
    fs::copy(guest_binary, &installed_guest_binary).with_context(|| {
        format!(
            "failed to inject sandbox guest binary into {}",
            installed_guest_binary.display()
        )
    })?;
    let mut perms = fs::metadata(&installed_guest_binary)
        .with_context(|| format!("failed to stat {}", installed_guest_binary.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&installed_guest_binary, perms).with_context(|| {
        format!(
            "failed to set executable mode on {}",
            installed_guest_binary.display()
        )
    })?;
    stage_guest_helper(rootfs_dir, GUEST_AGENT_ALIAS_PATH, agent_binary, "rkforge")?;
    stage_guest_helper(
        rootfs_dir,
        GUEST_INIT_ALIAS_PATH,
        guest_init_binary,
        "rkforge",
    )?;

    if !rootfs_dir.join("usr/local/bin/python3").is_file()
        && !rootfs_dir.join("usr/bin/python3").is_file()
        && !rootfs_dir.join("bin/python3").is_file()
    {
        warn!(
            rootfs=%rootfs_dir.display(),
            "python3 was not found in the guest rootfs; inline Python exec will fail until Python is installed in the sandbox image"
        );
    }

    Ok(())
}

fn stage_guest_helper(
    rootfs_dir: &Path,
    relative_path: &str,
    source_binary: &Path,
    fallback_target_name: &str,
) -> Result<()> {
    let alias_path = rootfs_dir.join(relative_path);
    if let Some(parent) = alias_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if alias_path.exists() || fs::symlink_metadata(&alias_path).is_ok() {
        fs::remove_file(&alias_path)
            .with_context(|| format!("failed to remove {}", alias_path.display()))?;
    }
    let main_canonical =
        fs::canonicalize(rootfs_dir.join(GUEST_RKFORGE_PATH)).with_context(|| {
            format!(
                "failed to resolve {}",
                rootfs_dir.join(GUEST_RKFORGE_PATH).display()
            )
        })?;
    let helper_canonical =
        fs::canonicalize(source_binary).unwrap_or_else(|_| source_binary.to_path_buf());
    if helper_canonical == main_canonical {
        std::os::unix::fs::symlink(fallback_target_name, &alias_path).with_context(|| {
            format!(
                "failed to create guest helper alias {}",
                alias_path.display()
            )
        })?;
    } else {
        fs::copy(source_binary, &alias_path).with_context(|| {
            format!(
                "failed to inject guest helper binary into {}",
                alias_path.display()
            )
        })?;
        let mut perms = fs::metadata(&alias_path)
            .with_context(|| format!("failed to stat {}", alias_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&alias_path, perms)
            .with_context(|| format!("failed to chmod {}", alias_path.display()))?;
    }
    Ok(())
}

fn build_ext4_image(rootfs_dir: &Path, output_path: &Path) -> Result<()> {
    let size_mb = rootfs_size_mb(rootfs_dir)? + GUEST_IMAGE_SIZE_PADDING_MIB;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let output = Command::new("mkfs.ext4")
        .arg("-q")
        .arg("-d")
        .arg(rootfs_dir)
        .arg("-F")
        .arg(output_path)
        .arg(format!("{size_mb}M"))
        .output()
        .with_context(|| {
            format!(
                "failed to execute mkfs.ext4 while building {}",
                output_path.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "mkfs.ext4 failed while building {}: {}",
            output_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    debug!(
        rootfs=%rootfs_dir.display(),
        output=%output_path.display(),
        size_mb,
        "built sandbox guest ext4 image"
    );
    Ok(())
}

fn rootfs_size_mb(rootfs_dir: &Path) -> Result<u64> {
    let total_bytes =
        WalkDir::new(rootfs_dir)
            .into_iter()
            .try_fold(0u64, |acc, entry| -> Result<u64> {
                let entry = entry.with_context(|| {
                    format!("failed to walk guest rootfs {}", rootfs_dir.display())
                })?;
                let metadata = entry.metadata().with_context(|| {
                    format!(
                        "failed to read metadata for guest rootfs entry {}",
                        entry.path().display()
                    )
                })?;
                Ok(acc + metadata.len())
            })?;
    Ok(total_bytes.div_ceil(1024 * 1024))
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn cache_key_uses_both_manifest_and_guest_binary_hash() {
        let key = cache_key(
            "0123456789abcdef0123456789abcdef",
            "fedcba9876543210fedcba9876543210",
        );
        assert_eq!(key, "0123456789abcdef-fedcba9876543210");
    }
}
