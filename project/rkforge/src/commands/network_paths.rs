use anyhow::{Result, anyhow};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const DEFAULT_NETAVARK_CONFIG_DIR: &str = "/run/containers/networks";
const AARDVARK_BINARY_NAME: &str = "aardvark-dns";
const SYSTEM_AARDVARK_CANDIDATES: [&str; 2] =
    ["/usr/libexec/podman/aardvark-dns", "/usr/bin/aardvark-dns"];

pub(crate) fn default_netavark_config_dir() -> OsString {
    std::env::var_os("NETAVARK_CONFIG")
        .unwrap_or_else(|| OsString::from(DEFAULT_NETAVARK_CONFIG_DIR))
}

pub(crate) fn default_aardvark_bin() -> Result<OsString> {
    if let Some(path) = std::env::var_os("AARDVARK_DNS_BIN") {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("AARDVARK_BIN") {
        return Ok(path);
    }

    for candidate in aardvark_candidates() {
        if candidate.is_file() {
            return Ok(candidate.into_os_string());
        }
    }

    Err(anyhow!(
        "aardvark-dns not found; set AARDVARK_DNS_BIN/AARDVARK_BIN, place it next to rkforge, or build it first with `cargo build -p aardvark-dns -p rkforge`"
    ))
}

fn aardvark_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = option_env!("RKFORGE_AARDVARK_DNS_BUILD_DEFAULT") {
        candidates.push(PathBuf::from(path));
    }

    if let Ok(exe_path) = std::env::current_exe() {
        append_exe_relative_candidates(&mut candidates, &exe_path);
    }

    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            path_entries(&path)
                .into_iter()
                .map(|dir| dir.join(AARDVARK_BINARY_NAME)),
        );
    }

    candidates.extend(SYSTEM_AARDVARK_CANDIDATES.into_iter().map(PathBuf::from));
    candidates
}

fn append_exe_relative_candidates(candidates: &mut Vec<PathBuf>, exe_path: &Path) {
    let Some(bin_dir) = exe_path.parent() else {
        return;
    };

    candidates.push(bin_dir.join(AARDVARK_BINARY_NAME));

    let Some(prefix_dir) = bin_dir.parent() else {
        return;
    };

    candidates.push(
        prefix_dir
            .join("libexec")
            .join("rkforge")
            .join(AARDVARK_BINARY_NAME),
    );
    candidates.push(
        prefix_dir
            .join("libexec")
            .join("podman")
            .join(AARDVARK_BINARY_NAME),
    );
}

fn path_entries(path: &OsString) -> Vec<PathBuf> {
    env::split_paths(path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn path_entries_are_searchable_for_aardvark() {
        let tmp = tempdir().unwrap();
        let bin = tmp.path().join(AARDVARK_BINARY_NAME);
        fs::write(&bin, b"#!/bin/sh\n").unwrap();

        let path = env::join_paths([tmp.path()]).unwrap();
        let found = path_entries(&path)
            .into_iter()
            .map(|dir| dir.join(AARDVARK_BINARY_NAME))
            .find(|candidate| candidate.is_file());

        assert_eq!(found, Some(bin));
    }
}
