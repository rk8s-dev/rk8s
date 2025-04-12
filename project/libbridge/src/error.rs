use cni_plugin::{error::CniError, reply::ErrorReply};
use semver::Version;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Cni(#[from] CniError),

    #[error(transparent)]
    Vlan(#[from] VlanError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("NetlinkError: {0}")]
    NetlinkError(String),

    #[error("NetnsError: {0}")]
    NetnsError(String),

    #[error("VethError: {0}")]
    VethError(String),

    #[error("LinkError: {0}")]
    LinkError(String),

    #[error(transparent)]
    AnyhowError(#[from] anyhow::Error),
}

impl AppError {
    pub fn into_reply(self, cni_version: Version) -> ErrorReply<'static> {
        match self {
            Self::Cni(e) => e.into_reply(cni_version),
            Self::Vlan(e) => e.into_reply(cni_version),
            AppError::InvalidConfig(msg) => ErrorReply {
                cni_version,
                code: 116,
                msg: "Invalid configuration",
                details: msg,
            },
            AppError::NetlinkError(msg) => ErrorReply {
                cni_version,
                code: 117,
                msg: "NetlinkError",
                details: msg,
            },
            AppError::NetnsError(msg) => ErrorReply {
                cni_version,
                code: 118,
                msg: "NetnsError",
                details: msg,
            },
            AppError::VethError(msg) => ErrorReply {
                cni_version,
                code: 119,
                msg: "VethError",
                details: msg,
            },
            AppError::LinkError(msg) => ErrorReply {
                cni_version,
                code: 120,
                msg: "LinkError",
                details: msg,
            },
            AppError::AnyhowError(err) => ErrorReply {
                cni_version,
                code: 121,
                msg: "Unknown error",
                details: err.to_string(),
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum VlanError {
    #[error("incorrect trunk minID parameter")]
    IncorrectMinID,

    #[error("incorrect trunk maxID parameter")]
    IncorrectMaxID,

    #[error("minID is greater than maxID in trunk parameter")]
    MinGreaterThanMax,

    #[error("minID and maxID should be configured simultaneously, minID is missing")]
    MissingMinID,

    #[error("minID and maxID should be configured simultaneously, maxID is missing")]
    MissingMaxID,

    #[error("incorrect trunk id parameter")]
    IncorrectTrunkID,
}

impl VlanError {
    pub fn into_reply(self, cni_version: Version) -> ErrorReply<'static> {
        ErrorReply {
            cni_version,
            code: 115,
            msg: "VLAN configuration error",
            details: self.to_string(),
        }
    }
}
