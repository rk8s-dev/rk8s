use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=TARGET");

    if let Some(path) = discover_aardvark_bin() {
        println!(
            "cargo:rustc-env=RKFORGE_AARDVARK_DNS_BUILD_DEFAULT={}",
            path.display()
        );
    }
}

fn discover_aardvark_bin() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir.parent()?;
    let target_dir = cargo_target_dir(workspace_root);
    let profile = env::var("PROFILE").ok()?;
    let target = env::var("TARGET").ok()?;

    [
        target_dir.join(&profile).join("aardvark-dns"),
        target_dir.join(&target).join(&profile).join("aardvark-dns"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn cargo_target_dir(workspace_root: &Path) -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR") {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        }
        None => workspace_root.join("target"),
    }
}
