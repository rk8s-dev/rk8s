use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildSecret {
    pub id: String,
    pub src: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildSshAgent {
    pub id: String,
    pub socket_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildHostEntry {
    pub host: String,
    pub ip: IpAddr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildUlimitResource {
    Core,
    Cpu,
    Data,
    Fsize,
    Nofile,
    Nproc,
    Stack,
    As,
    Memlock,
}

impl BuildUlimitResource {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "core" => Some(Self::Core),
            "cpu" => Some(Self::Cpu),
            "data" => Some(Self::Data),
            "fsize" => Some(Self::Fsize),
            "nofile" => Some(Self::Nofile),
            "nproc" => Some(Self::Nproc),
            "stack" => Some(Self::Stack),
            "as" => Some(Self::As),
            "memlock" => Some(Self::Memlock),
            _ => None,
        }
    }

    pub fn as_name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Cpu => "cpu",
            Self::Data => "data",
            Self::Fsize => "fsize",
            Self::Nofile => "nofile",
            Self::Nproc => "nproc",
            Self::Stack => "stack",
            Self::As => "as",
            Self::Memlock => "memlock",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildUlimitValue {
    Unlimited,
    Value(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildUlimit {
    pub resource: BuildUlimitResource,
    pub soft: BuildUlimitValue,
    pub hard: BuildUlimitValue,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
pub enum BuildNetworkMode {
    #[default]
    Default,
    None,
    Host,
}

impl BuildNetworkMode {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::None => "none",
            Self::Host => "host",
        }
    }
}

pub fn normalize_cgroup_parent(raw: &str) -> Result<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("--cgroup-parent must not be empty");
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir | Component::RootDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                bail!("--cgroup-parent must not contain `..`");
            }
            Component::Prefix(_) => {
                bail!("--cgroup-parent has unsupported path prefix");
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        bail!("--cgroup-parent resolved to an empty path");
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_cgroup_parent;

    #[test]
    fn test_normalize_cgroup_parent() {
        assert_eq!(
            normalize_cgroup_parent("rkforge/build")
                .unwrap()
                .to_string_lossy(),
            "rkforge/build"
        );
        assert_eq!(
            normalize_cgroup_parent("./rkforge//build")
                .unwrap()
                .to_string_lossy(),
            "rkforge/build"
        );
        assert!(normalize_cgroup_parent("").is_err());
        assert!(normalize_cgroup_parent("../rkforge").is_err());
    }
}
