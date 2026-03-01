use anyhow::Result;
use std::{env, fs, path::Path, path::PathBuf};
use uuid::Uuid;

/// Get the path to the busybox image for testing
#[allow(dead_code)]
pub fn bundles_path(image_name: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("test")
        .join("bundles")
        .join(image_name);
    root.to_string_lossy().to_string()
}

/// Create temporary compose configuration file
#[allow(dead_code)]
pub fn create_temp_compose_file(content: &str) -> Result<String> {
    let temp_dir = env::temp_dir();
    let compose_path = temp_dir.join(format!("test-compose-{}.yaml", Uuid::new_v4()));
    fs::write(&compose_path, content)?;
    Ok(compose_path.to_str().unwrap().to_string())
}

/// Clean up temporary compose configuration file
#[allow(dead_code)]
pub fn cleanup_temp_compose_file(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Clean up compose project
#[allow(dead_code)]
pub fn cleanup_compose_project(project_name: &str) -> Result<()> {
    let compose_dir = Path::new("/run/youki/compose").join(project_name);
    if compose_dir.exists() {
        fs::remove_dir_all(compose_dir)?;
    }
    Ok(())
}

/// Clean up container
#[allow(dead_code)]
pub fn cleanup_container(container_id: &str) -> Result<()> {
    let container_dir = Path::new("/run/youki").join(container_id);
    if container_dir.exists() {
        fs::remove_dir_all(container_dir)?;
    }
    Ok(())
}

/// Clean up volume
#[allow(dead_code)]
pub fn cleanup_volume(volume_name: &str) -> Result<()> {
    let volume_dir = Path::new("/var/lib/rkl/volumes").join(volume_name);
    if volume_dir.exists() {
        fs::remove_dir_all(volume_dir)?;
    }
    Ok(())
}

/// Clean up pod
#[allow(dead_code)]
pub fn cleanup_pod(pod_name: &str) -> Result<()> {
    let pod_dir = Path::new("/run/youki/pods").join(pod_name);
    if pod_dir.exists() {
        fs::remove_dir_all(pod_dir)?;
    }
    Ok(())
}
