use anyhow::{Context, Result, bail};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

mod embedded_runtime_manifest {
    include!(concat!(env!("OUT_DIR"), "/sandbox_runtime_manifest.rs"));
}

const RUNTIME_DIR_OVERRIDE_ENV: &str = "RKFORGE_RUNTIME_DIR";
const HOST_BINARY_OVERRIDE_ENV: &str = "RKFORGE_SANDBOX_HOST_BIN";
const LIBKRUN_LIBRARY_OVERRIDE_ENV: &str = "RKFORGE_LIBKRUN_LIBRARY";
const LIBKRUN_DIR_OVERRIDE_ENV: &str = "RKFORGE_LIBKRUN_DIR";
const LIBKRUNFW_LIBRARY_OVERRIDE_ENV: &str = "RKFORGE_LIBKRUNFW_PATH";
const LIBKRUNFW_DIR_OVERRIDE_ENV: &str = "RKFORGE_LIBKRUNFW_DIR";
pub const RUNTIME_BIN_NAME: &str = "rkforge";
pub const SHIM_HELPER_BIN_NAME: &str = "rkforge-sandbox-shim";
pub const AGENT_HELPER_BIN_NAME: &str = "rkforge-sandbox-agent";
pub const GUEST_INIT_HELPER_BIN_NAME: &str = "rkforge-sandbox-guest-init";
const REPO_RUNTIME_DIR_NAME: &str = "runtime";
const REPO_RUNTIME_CURRENT_DIR_NAME: &str = "current";

#[derive(Debug, Clone)]
pub struct RuntimeAssetBundle {
    rkforge_binary: PathBuf,
    shim_binary: PathBuf,
    agent_binary: PathBuf,
    guest_init_binary: PathBuf,
    libkrun_library: Option<PathBuf>,
    libkrunfw_path: Option<PathBuf>,
}

impl RuntimeAssetBundle {
    pub fn prepare(root: &Path) -> Result<Self> {
        if let Some(source_bundle_root) = discover_source_runtime_root() {
            let staged_root = root.join("runtime");
            stage_runtime_bundle(&source_bundle_root, &staged_root)?;
            return Self::open_existing(staged_root);
        }

        let runtime_root = root.join("runtime");
        let bin_dir = runtime_root.join("bin");
        let lib_dir = runtime_root.join("lib");
        fs::create_dir_all(&bin_dir)
            .with_context(|| format!("failed to create {}", bin_dir.display()))?;
        fs::create_dir_all(&lib_dir)
            .with_context(|| format!("failed to create {}", lib_dir.display()))?;

        let (
            rkforge_binary,
            shim_binary,
            agent_binary,
            guest_init_binary,
            libkrun_library,
            libkrunfw_path,
        ) = if extract_embedded_runtime_assets(&runtime_root)? {
            if !runtime_root.join("bin").join(RUNTIME_BIN_NAME).is_file() {
                let host_binary = std::env::var_os(HOST_BINARY_OVERRIDE_ENV)
                    .map(PathBuf::from)
                    .unwrap_or(
                        std::env::current_exe().context("failed to resolve current executable")?,
                    );
                let rkforge_binary = bin_dir.join(RUNTIME_BIN_NAME);
                stage_executable(&host_binary, &rkforge_binary)?;
                stage_helper_aliases(&bin_dir, RUNTIME_BIN_NAME)?;
            }
            resolve_bundle_layout(&runtime_root)?
        } else {
            let host_binary = std::env::var_os(HOST_BINARY_OVERRIDE_ENV)
                .map(PathBuf::from)
                .unwrap_or(
                    std::env::current_exe().context("failed to resolve current executable")?,
                );
            if !host_binary.is_file() {
                bail!(
                    "sandbox host binary does not exist: {}",
                    host_binary.display()
                );
            }
            let rkforge_binary = bin_dir.join(RUNTIME_BIN_NAME);
            stage_executable(&host_binary, &rkforge_binary)?;
            stage_helper_aliases(&bin_dir, RUNTIME_BIN_NAME)?;

            let (libkrun_library, libkrunfw_path) = (
                if let Some(path) = discover_libkrun_library().as_deref() {
                    Some(stage_shared_library(path, &lib_dir, "libkrun.so")?)
                } else {
                    None
                },
                if let Some(path) =
                    discover_libkrunfw_path(discover_libkrun_library().as_deref()).as_deref()
                {
                    Some(stage_shared_library(path, &lib_dir, "libkrunfw.so")?)
                } else {
                    None
                },
            );
            let (rkforge_binary, shim_binary, agent_binary, guest_init_binary, _, _) =
                resolve_bundle_layout(&runtime_root)?;
            (
                rkforge_binary,
                shim_binary,
                agent_binary,
                guest_init_binary,
                libkrun_library,
                libkrunfw_path,
            )
        };

        Ok(Self {
            rkforge_binary,
            shim_binary,
            agent_binary,
            guest_init_binary,
            libkrun_library,
            libkrunfw_path,
        })
    }

    pub fn discover_from_current_process() -> Result<Option<Self>> {
        if let Some(root) = current_process_bundle_root() {
            return Self::open_existing(root).map(Some);
        }
        if let Some(root) = discover_repo_runtime_root() {
            return Self::open_existing(root).map(Some);
        }
        Ok(None)
    }

    pub fn rkforge_binary(&self) -> &Path {
        &self.rkforge_binary
    }

    pub fn shim_binary(&self) -> &Path {
        &self.shim_binary
    }

    pub fn agent_binary(&self) -> &Path {
        &self.agent_binary
    }

    pub fn guest_init_binary(&self) -> &Path {
        &self.guest_init_binary
    }

    pub fn libkrun_library(&self) -> Option<&Path> {
        self.libkrun_library.as_deref()
    }

    pub fn libkrunfw_path(&self) -> Option<&Path> {
        self.libkrunfw_path.as_deref()
    }

    fn open_existing(root: PathBuf) -> Result<Self> {
        let (
            rkforge_binary,
            shim_binary,
            agent_binary,
            guest_init_binary,
            libkrun_library,
            libkrunfw_path,
        ) = resolve_bundle_layout(&root)?;
        Ok(Self {
            rkforge_binary,
            shim_binary,
            agent_binary,
            guest_init_binary,
            libkrun_library,
            libkrunfw_path,
        })
    }
}

pub fn discover_libkrun_library() -> Option<PathBuf> {
    if let Some(path) = env_file_path(LIBKRUN_LIBRARY_OVERRIDE_ENV) {
        return Some(path);
    }
    if let Some(dir) = env_dir_path(LIBKRUN_DIR_OVERRIDE_ENV)
        && let Some(path) = find_library_in_dir(&dir, &["libkrun.so"])
    {
        return Some(path);
    }
    for dir in standard_library_dirs() {
        if let Some(path) = find_library_in_dir(&dir, &["libkrun.so"]) {
            return Some(path);
        }
    }
    None
}

pub fn discover_libkrunfw_path(libkrun_library: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = env_file_path(LIBKRUNFW_LIBRARY_OVERRIDE_ENV) {
        return Some(path);
    }
    if let Some(dir) = env_dir_path(LIBKRUNFW_DIR_OVERRIDE_ENV)
        && let Some(path) = find_library_in_dir(&dir, &["libkrunfw.so"])
    {
        return Some(path);
    }
    if let Some(dir) = libkrun_library.and_then(Path::parent)
        && let Some(path) = find_library_in_dir(dir, &["libkrunfw.so"])
    {
        return Some(path);
    }
    for dir in standard_library_dirs() {
        if let Some(path) = find_library_in_dir(&dir, &["libkrunfw.so"]) {
            return Some(path);
        }
    }
    None
}

fn runtime_dir_override() -> Option<PathBuf> {
    let path = std::env::var_os(RUNTIME_DIR_OVERRIDE_ENV).map(PathBuf::from)?;
    is_runtime_bundle_root(&path).then_some(path)
}

fn current_process_bundle_root() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let bin_dir = current_exe.parent()?;
    if bin_dir.file_name().and_then(|name| name.to_str()) != Some("bin") {
        return None;
    }
    let root = bin_dir.parent()?;
    is_runtime_bundle_root(root).then_some(root.to_path_buf())
}

fn discover_source_runtime_root() -> Option<PathBuf> {
    runtime_dir_override()
        .or_else(current_process_bundle_root)
        .or_else(discover_repo_runtime_root)
}

fn discover_repo_runtime_root() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let mut dir = current_exe.parent()?.to_path_buf();
    loop {
        let candidates = [
            dir.join(REPO_RUNTIME_DIR_NAME),
            dir.join(REPO_RUNTIME_DIR_NAME)
                .join(REPO_RUNTIME_CURRENT_DIR_NAME),
        ];
        for candidate in candidates {
            if is_runtime_bundle_root(&candidate) {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn is_runtime_bundle_root(path: &Path) -> bool {
    path.join("bin").join(RUNTIME_BIN_NAME).is_file() && path.join("lib").is_dir()
}

#[allow(clippy::type_complexity)]
fn resolve_bundle_layout(
    root: &Path,
) -> Result<(
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    Option<PathBuf>,
    Option<PathBuf>,
)> {
    let bin_dir = root.join("bin");
    let lib_dir = root.join("lib");
    let rkforge_binary = bin_dir.join(RUNTIME_BIN_NAME);
    if !rkforge_binary.is_file() {
        bail!(
            "sandbox runtime bundle is missing binary {}",
            rkforge_binary.display()
        );
    }
    let shim_binary = helper_or_main(&bin_dir, SHIM_HELPER_BIN_NAME, &rkforge_binary);
    let agent_binary = helper_or_main(&bin_dir, AGENT_HELPER_BIN_NAME, &rkforge_binary);
    let guest_init_binary = helper_or_main(&bin_dir, GUEST_INIT_HELPER_BIN_NAME, &rkforge_binary);
    let libkrun_library = find_library_in_dir(&lib_dir, &["libkrun.so"]);
    let libkrunfw_path = find_library_in_dir(&lib_dir, &["libkrunfw.so"]);
    Ok((
        rkforge_binary,
        shim_binary,
        agent_binary,
        guest_init_binary,
        libkrun_library,
        libkrunfw_path,
    ))
}

fn helper_or_main(bin_dir: &Path, helper_name: &str, main_binary: &Path) -> PathBuf {
    let helper = bin_dir.join(helper_name);
    if helper.is_file() {
        helper
    } else {
        main_binary.to_path_buf()
    }
}

fn stage_runtime_bundle(source_root: &Path, dest_root: &Path) -> Result<()> {
    let source_bin = source_root.join("bin");
    let source_lib = source_root.join("lib");
    let dest_bin = dest_root.join("bin");
    let dest_lib = dest_root.join("lib");
    fs::create_dir_all(&dest_bin)
        .with_context(|| format!("failed to create {}", dest_bin.display()))?;
    fs::create_dir_all(&dest_lib)
        .with_context(|| format!("failed to create {}", dest_lib.display()))?;

    stage_executable(
        &source_bin.join(RUNTIME_BIN_NAME),
        &dest_bin.join(RUNTIME_BIN_NAME),
    )?;
    stage_directory_contents(&source_lib, &dest_lib)?;
    if source_bin.is_dir() {
        stage_directory_contents(&source_bin, &dest_bin)?;
    }
    stage_helper_aliases(&dest_bin, RUNTIME_BIN_NAME)?;
    Ok(())
}

fn extract_embedded_runtime_assets(runtime_root: &Path) -> Result<bool> {
    if embedded_runtime_manifest::EMBEDDED_RUNTIME_ASSETS.is_empty() {
        return Ok(false);
    }
    fs::create_dir_all(runtime_root)
        .with_context(|| format!("failed to create {}", runtime_root.display()))?;
    for (relative_path, bytes, mode) in embedded_runtime_manifest::EMBEDDED_RUNTIME_ASSETS {
        let relative = Path::new(relative_path);
        let dest = runtime_root.join(relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&dest, bytes).with_context(|| {
            format!(
                "failed to extract embedded runtime asset {}",
                dest.display()
            )
        })?;
        let mut perms = fs::metadata(&dest)
            .with_context(|| format!("failed to stat {}", dest.display()))?
            .permissions();
        perms.set_mode(*mode);
        fs::set_permissions(&dest, perms)
            .with_context(|| format!("failed to chmod {}", dest.display()))?;
    }
    let bin_dir = runtime_root.join("bin");
    if bin_dir.is_dir() {
        stage_helper_aliases(&bin_dir, RUNTIME_BIN_NAME)?;
    }
    Ok(true)
}

fn stage_helper_aliases(bin_dir: &Path, main_name: &str) -> Result<()> {
    for helper in [
        SHIM_HELPER_BIN_NAME,
        AGENT_HELPER_BIN_NAME,
        GUEST_INIT_HELPER_BIN_NAME,
    ] {
        let alias_path = bin_dir.join(helper);
        let target_path = bin_dir.join(main_name);
        if alias_path == target_path {
            continue;
        }
        if alias_path.is_file() {
            continue;
        }
        replace_symlink_or_file(&alias_path)?;
        symlink(main_name, &alias_path)
            .with_context(|| format!("failed to create helper alias {}", alias_path.display()))?;
    }
    Ok(())
}

fn stage_directory_contents(source_dir: &Path, dest_dir: &Path) -> Result<()> {
    let entries = fs::read_dir(source_dir)
        .with_context(|| format!("failed to read runtime directory {}", source_dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to iterate runtime directory {}",
                source_dir.display()
            )
        })?;
        let source_path = entry.path();
        let dest_path = dest_dir.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .with_context(|| format!("failed to stat {}", source_path.display()))?;
        replace_symlink_or_file(&dest_path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_path)
                .with_context(|| format!("failed to read link {}", source_path.display()))?;
            symlink(&target, &dest_path).with_context(|| {
                format!(
                    "failed to stage symlink {} into {}",
                    source_path.display(),
                    dest_path.display()
                )
            })?;
        } else if metadata.is_file() {
            stage_file(&source_path, &dest_path)?;
        }
    }
    Ok(())
}

fn stage_executable(source: &Path, dest: &Path) -> Result<()> {
    stage_file(source, dest)?;
    let mut perms = fs::metadata(dest)
        .with_context(|| format!("failed to stat {}", dest.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(dest, perms)
        .with_context(|| format!("failed to chmod {}", dest.display()))?;
    Ok(())
}

fn stage_shared_library(source: &Path, lib_dir: &Path, base_name: &str) -> Result<PathBuf> {
    let resolved = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let actual_name = resolved
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid shared library path {}", resolved.display()))?;
    let staged_actual = lib_dir.join(actual_name);
    stage_file(&resolved, &staged_actual)?;

    let mut aliases = vec![base_name.to_string()];
    if let Some(source_name) = source.file_name().and_then(|name| name.to_str())
        && source_name != base_name
        && source_name != actual_name.to_string_lossy()
    {
        aliases.push(source_name.to_string());
    }
    if let Some(major_alias) = major_alias_name(base_name, &resolved)
        && !aliases.iter().any(|alias| alias == &major_alias)
    {
        aliases.push(major_alias);
    }

    for alias in aliases {
        let alias_path = lib_dir.join(&alias);
        if alias_path == staged_actual {
            continue;
        }
        replace_symlink_or_file(&alias_path)?;
        symlink(actual_name, &alias_path)
            .with_context(|| format!("failed to create symlink {}", alias_path.display()))?;
    }

    Ok(lib_dir.join(base_name))
}

fn stage_file(source: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = dest.with_extension(format!("tmp-{}", std::process::id()));
    fs::copy(source, &tmp).with_context(|| {
        format!(
            "failed to copy runtime asset from {} to {}",
            source.display(),
            tmp.display()
        )
    })?;
    fs::rename(&tmp, dest).with_context(|| {
        format!(
            "failed to move staged runtime asset into place {}",
            dest.display()
        )
    })?;
    Ok(())
}

fn replace_symlink_or_file(path: &Path) -> Result<()> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat runtime asset {}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn major_alias_name(base_name: &str, resolved: &Path) -> Option<String> {
    let file_name = resolved.file_name()?.to_str()?;
    let suffix = file_name.strip_prefix(&format!("{base_name}."))?;
    let major = suffix.split('.').next()?;
    Some(format!("{base_name}.{major}"))
}

fn env_file_path(key: &str) -> Option<PathBuf> {
    let path = std::env::var_os(key).map(PathBuf::from)?;
    path.is_file().then_some(path)
}

fn env_dir_path(key: &str) -> Option<PathBuf> {
    let path = std::env::var_os(key).map(PathBuf::from)?;
    path.is_dir().then_some(path)
}

fn standard_library_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/local/lib64"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/usr/lib"),
        PathBuf::from("/lib64"),
        PathBuf::from("/lib"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/lib64"));
        dirs.push(home.join(".local/lib"));
    }
    dirs
}

fn find_library_in_dir(dir: &Path, prefixes: &[&str]) -> Option<PathBuf> {
    for prefix in prefixes {
        let direct = dir.join(prefix);
        if direct.is_file() {
            return Some(direct);
        }
    }

    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if prefixes.iter().any(|prefix| name.starts_with(prefix)) && path.is_file() {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{is_runtime_bundle_root, major_alias_name};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn major_alias_is_derived_from_resolved_library_name() {
        let alias = major_alias_name("libkrunfw.so", Path::new("/tmp/libkrunfw.so.5.3.0"));
        assert_eq!(alias.as_deref(), Some("libkrunfw.so.5"));
    }

    #[test]
    fn runtime_bundle_root_requires_bin_and_lib_layout() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        fs::create_dir_all(dir.path().join("lib")).unwrap();
        fs::write(dir.path().join("bin/rkforge"), b"").unwrap();
        assert!(is_runtime_bundle_root(dir.path()));
    }
}
