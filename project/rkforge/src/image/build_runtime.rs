use serde::{Deserialize, Serialize};
use std::net::IpAddr;

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
