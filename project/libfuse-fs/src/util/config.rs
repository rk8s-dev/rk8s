// ... existing code ...
use std::collections::HashMap;
use ceres::lfs;
use serde::Deserialize;
use toml::Table;
use std::path::PathBuf;
use std::{fs, io};

static CONFIG:&str = "config.toml";

// Read the scorpio (config)TOML file and return a Table.
#[inline]
fn read_toml() -> Table{
    
    let config_content = std::fs::read_to_string(CONFIG).expect("Unable to read config file");
    toml::de::from_str(&config_content).expect("Unable to parse TOML")

}

// Get the `store_path` from the TOML file and return a PathBuf.
pub fn store_path() -> PathBuf{
    let config = read_toml();
    let store_path = config.get("store_path").expect("store_path not found in config").as_str().unwrap();
    

    let mut store_path_buf = PathBuf::from(store_path);
    
    if !store_path_buf.exists() {
        fs::create_dir_all(&store_path_buf).expect("Failed to create directory");
    }
    store_path_buf
}

pub fn mount_path() -> PathBuf{
    let config = read_toml();
    let mount_path = config.get("mount_path").expect("mount_path not found in config").as_str().unwrap();
    PathBuf::from(mount_path)
}
pub fn git_branch() -> Result<String,io::Error>{
    let config = read_toml();
    let git_branch = config.get("git_branch").expect("git_branch not found in config").as_str()?;
    Ok(git_branch.to_string())
}