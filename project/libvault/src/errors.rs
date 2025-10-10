//! The `libvault::errors` module defines an enumeration of various error code, and implements
//! neccessary traits against it.
//!
//! The error code defined in this module are used widely in RustyVault.

use std::{
    io,
    sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard},
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RvError {
    #[error("Cipher operation update failed.")]
    ErrCryptoCipherUpdateFailed,
    #[error("Cipher operation finalization failed.")]
    ErrCryptoCipherFinalizeFailed,
    #[error("Cipher initialization failed.")]
    ErrCryptoCipherInitFailed,
    #[error("Cipher not initialized.")]
    ErrCryptoCipherNotInited,
    #[error("Cipher operation not supported.")]
    ErrCryptoCipherOPNotSupported,
    #[error("AEAD Cipher tag is missing.")]
    ErrCryptoCipherNoTag,
    #[error("AEAD Cipher tag should not be present.")]
    ErrCryptoCipherAEADTagPresent,
    #[error("Config path is invalid.")]
    ErrConfigPathInvalid,
    #[error("Config load failed.")]
    ErrConfigLoadFailed,
    #[error("Config storage not found.")]
    ErrConfigStorageNotFound,
    #[error("Config listener not found.")]
    ErrConfigListenerNotFound,
    #[error("Core is not initialized.")]
    ErrCoreNotInit,
    #[error("Core logical backend already exists.")]
    ErrCoreLogicalBackendExist,
    #[error("Core logical backend does not exist.")]
    ErrCoreLogicalBackendNoExist,
    #[error("Core router not handling.")]
    ErrCoreRouterNotHandling,
    #[error("Core handler already exists.")]
    ErrCoreHandlerExist,
    #[error("Core seal config is invalid.")]
    ErrCoreSealConfigInvalid,
    #[error("Core seal config not found.")]
    ErrCoreSealConfigNotFound,
    #[error("Core unseal key set not found.")]
    ErrCoreDeprecatedUnsealKeySetNotFound,
    #[error("Physical configuration item is missing.")]
    ErrPhysicalConfigItemMissing,
    #[error("Physical type is invalid.")]
    ErrPhysicalTypeInvalid,
    #[error("Physical backend prefix is invalid.")]
    ErrPhysicalBackendPrefixInvalid,
    #[error("Physical backend key is invalid.")]
    ErrPhysicalBackendKeyInvalid,
    #[error("RustyVault key sanity check failed.")]
    ErrBarrierKeySanityCheckFailed,
    #[error("RustyVault is already initialized.")]
    ErrBarrierAlreadyInit,
    #[error("RustyVault unseal key is invalid.")]
    ErrBarrierKeyInvalid,
    #[error("RustyVault unseal key is deprecated.")]
    ErrBarrierKeyDeprecated,
    #[error("RustyVault is not initialized.")]
    ErrBarrierNotInit,
    #[error("RustyVault is sealed.")]
    ErrBarrierSealed,
    #[error("RustyVault is unsealing.")]
    ErrBarrierUnsealing,
    #[error("RustyVault is unsealed.")]
    ErrBarrierUnsealed,
    #[error("RustyVault unseal failed.")]
    ErrBarrierUnsealFailed,
    #[error("RustyVualt barrier epoch do not match.")]
    ErrBarrierEpochMismatch,
    #[error("RustyVault barrier version do not match.")]
    ErrBarrierVersionMismatch,
    #[error("RustyVault barrier key generation failed.")]
    ErrBarrierKeyGenerationFailed,
    #[error("Router mount conflict.")]
    ErrRouterMountConflict,
    #[error("Router mount not found.")]
    ErrRouterMountNotFound,
    #[error("Mount path is failed, cannot mount.")]
    ErrMountFailed,
    #[error("Mount path is protected, cannot mount.")]
    ErrMountPathProtected,
    #[error("Mount path already exists.")]
    ErrMountPathExist,
    #[error("Mount table not found.")]
    ErrMountTableNotFound,
    #[error("Mount table is not ready.")]
    ErrMountTableNotReady,
    #[error("Mount not match.")]
    ErrMountNotMatch,
    #[error("Logical backend path not supported.")]
    ErrLogicalPathUnsupported,
    #[error("Logical backend operation not supported.")]
    ErrLogicalOperationUnsupported,
    #[error("Request is not ready.")]
    ErrRequestNotReady,
    #[error("No data is available for the request.")]
    ErrRequestNoData,
    #[error("No data field is available for the request.")]
    ErrRequestNoDataField,
    #[error("Request is invalid.")]
    ErrRequestInvalid,
    #[error("Request client token is missing.")]
    ErrRequestClientTokenMissing,
    #[error("Request field is not found.")]
    ErrRequestFieldNotFound,
    #[error("Request field is invalid.")]
    ErrRequestFieldInvalid,
    #[error("Response data is invalid.")]
    ErrResponseDataInvalid,
    #[error("Handler is default.")]
    ErrHandlerDefault,
    #[error("Module kv data field is missing.")]
    ErrModuleKvDataFieldMissing,
    #[error("Rust downcast failed.")]
    ErrRustDowncastFailed,
    #[error("Shamir share count invalid.")]
    ErrShamirShareCountInvalid,
    #[error("Module conflict.")]
    ErrModuleConflict,
    #[error("Module is not init.")]
    ErrModuleNotInit,
    #[error("Module is not found.")]
    ErrModuleNotFound,
    #[error("Auth module is disabled.")]
    ErrAuthModuleDisabled,
    #[error("Auth token is not found.")]
    ErrAuthTokenNotFound,
    #[error("Auth token id is invalid.")]
    ErrAuthTokenIdInvalid,
    #[error("Lease is not found.")]
    ErrLeaseNotFound,
    #[error("Lease is not renewable.")]
    ErrLeaseNotRenewable,
    #[error("Permission denied.")]
    ErrPermissionDenied,
    #[error("PKI pem bundle is invalid.")]
    ErrPkiPemBundleInvalid,
    #[error("PKI ca public key of certificate does not match private key.")]
    ErrPkiCertKeyMismatch,
    #[error("PKI cert chain is incorrect.")]
    ErrPkiCertChainIncorrect,
    #[error("PKI cert is not ca.")]
    ErrPkiCertIsNotCA,
    #[error("PKI ca private key is not found.")]
    ErrPkiCaKeyNotFound,
    #[error("PKI ca is not config.")]
    ErrPkiCaNotConfig,
    #[error("PKI ca extension is incorrect.")]
    ErrPkiCaExtensionIncorrect,
    #[error("PKI key type is invalid.")]
    ErrPkiKeyTypeInvalid,
    #[error("PKI key bits is invalid.")]
    ErrPkiKeyBitsInvalid,
    #[error("PKI key_name already exists.")]
    ErrPkiKeyNameAlreadyExist,
    #[error("PKI key operation is invalid.")]
    ErrPkiKeyOperationInvalid,
    #[error("PKI certificate is not found.")]
    ErrPkiCertNotFound,
    #[error("PKI role is not found.")]
    ErrPkiRoleNotFound,
    #[error("PKI data is invalid.")]
    ErrPkiDataInvalid,
    #[error("PKI internal error.")]
    ErrPkiInternal,
    #[error("Credentail is invalid.")]
    ErrCredentailInvalid,
    #[error("Credentail is not config.")]
    ErrCredentailNotConfig,
    #[error("Storage backend doesn't require a lock.")]
    ErrStorageBackendLockless,
    #[error("Storage backend lock failed.")]
    ErrStorageBackendLockFailed,
    #[error("Storage backend unlock failed.")]
    ErrStorageBackendUnlockFailed,
    #[error("Some IO error happened, {:?}", .source)]
    IO {
        #[from]
        source: io::Error,
    },
    #[error("Some serde_json error happened, {:?}", .source)]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },
    #[error("Some serde_yaml error happened, {:?}", .source)]
    SerdeYaml {
        #[from]
        source: serde_yaml::Error,
    },
    #[error("HTTP client error: {source}")]
    Reqwest {
        #[from]
        source: reqwest::Error,
    },
    #[error("Some openssl error happened, {:?}", .source)]
    OpenSSL {
        #[from]
        source: openssl::error::ErrorStack,
    },
    #[error("Some pem error happened, {:?}", .source)]
    Pem {
        #[from]
        source: pem::PemError,
    },
    #[error("Some regex error happened, {:?}", .source)]
    Regex {
        #[from]
        source: regex::Error,
    },
    #[error("Some hex error happened, {:?}", .source)]
    Hex {
        #[from]
        source: hex::FromHexError,
    },
    #[error("Some hcl error happened, {:?}", .source)]
    Hcl {
        #[from]
        source: hcl::Error,
    },
    #[error("Some humantime duration error happened, {:?}", .source)]
    HumantimeDuration {
        #[from]
        source: humantime::DurationError,
    },
    #[error("Some humantime timestamp error happened, {:?}", .source)]
    HumantimeTimestamp {
        #[from]
        source: humantime::TimestampError,
    },
    #[error("Some system_time error happened, {:?}", .source)]
    SystemTimeError {
        #[from]
        source: std::time::SystemTimeError,
    },
    #[error("Some chrono error happened, {:?}", .source)]
    ChronoError {
        #[from]
        source: chrono::ParseError,
    },
    #[error("Some bcrypt error happened, {:?}", .source)]
    BcryptError {
        #[from]
        source: bcrypt::BcryptError,
    },
    #[error("Some ureq error happened, {:?}", .source)]
    UreqError {
        #[from]
        source: ureq::Error,
    },
    #[error("RwLock was poisoned (reading)")]
    ErrRwLockReadPoison,
    #[error("RwLock was poisoned (writing)")]
    ErrRwLockWritePoison,

    #[error("Some net addr parse error happened, {:?}", .source)]
    AddrParseError {
        #[from]
        source: std::net::AddrParseError,
    },
    #[error("Some ipnetwork error happened, {:?}", .source)]
    IpNetworkError {
        #[from]
        source: ipnetwork::IpNetworkError,
    },

    #[error("Some url error happened, {:?}", .source)]
    UrlError {
        #[from]
        source: url::ParseError,
    },

    #[error("Some rustls error happened, {:?}", .source)]
    RustlsError {
        #[from]
        source: rustls::Error,
    },

    #[error("Some rustls_pemfile error happened")]
    RustlsPemFileError(rustls_pemfile::Error),

    #[error("Some rustls_pki_types error happened")]
    RustlsPkiTypesPemFileError(rustls::pki_types::pem::Error),

    #[error("Some tokio task error happened")]
    TokioTaskJoinError {
        #[from]
        source: tokio::task::JoinError,
    },

    #[error("Some string utf8 error happened, {:?}", .source)]
    StringUtf8Error {
        #[from]
        source: std::string::FromUtf8Error,
    },

    #[error("Some lockfile error happened, {:?}", .source)]
    LockfileError {
        #[from]
        source: lockfile::Error,
    },

    #[error("Database connection info invalid")]
    ErrDatabaseConnectionInfoInvalid,

    #[error("Some etcd client error happened, {:?}", .source)]
    EtcdClientError {
        #[from]
        source: etcd_client::Error,
    },

    #[error(transparent)]
    ErrOther(#[from] anyhow::Error),
    #[error("Some error happend, response text: {0}")]
    ErrResponse(String),
    #[error("Some error happend, status: {0}, response text: {1}")]
    ErrResponseStatus(u16, String),
    #[error("{0}")]
    ErrString(String),
    #[error("Unknown error.")]
    ErrUnknown,
}

impl RvError {
    pub fn response_status(&self) -> u16 {
        match self {
            RvError::ErrRequestNoData
            | RvError::ErrBarrierAlreadyInit
            | RvError::ErrBarrierKeyInvalid
            | RvError::ErrBarrierNotInit
            | RvError::ErrBarrierUnsealed
            | RvError::ErrBarrierUnsealFailed
            | RvError::ErrRequestNoDataField
            | RvError::ErrRequestInvalid
            | RvError::ErrRequestClientTokenMissing
            | RvError::ErrRequestFieldNotFound
            | RvError::ErrRequestFieldInvalid => 400,
            RvError::ErrBarrierSealed => 503,
            RvError::ErrPermissionDenied => 403,
            RvError::ErrRouterMountNotFound => 404,
            _ => 500,
        }
    }
}

impl PartialEq for RvError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RvError::ErrCryptoCipherUpdateFailed, RvError::ErrCryptoCipherUpdateFailed)
            | (RvError::ErrCryptoCipherFinalizeFailed, RvError::ErrCryptoCipherFinalizeFailed)
            | (RvError::ErrCryptoCipherInitFailed, RvError::ErrCryptoCipherInitFailed)
            | (RvError::ErrCryptoCipherNotInited, RvError::ErrCryptoCipherNotInited)
            | (RvError::ErrCryptoCipherOPNotSupported, RvError::ErrCryptoCipherOPNotSupported)
            | (RvError::ErrCryptoCipherNoTag, RvError::ErrCryptoCipherNoTag)
            | (RvError::ErrCryptoCipherAEADTagPresent, RvError::ErrCryptoCipherAEADTagPresent)
            | (RvError::ErrCoreLogicalBackendExist, RvError::ErrCoreLogicalBackendExist)
            | (RvError::ErrCoreNotInit, RvError::ErrCoreNotInit)
            | (RvError::ErrCoreLogicalBackendNoExist, RvError::ErrCoreLogicalBackendNoExist)
            | (RvError::ErrCoreSealConfigInvalid, RvError::ErrCoreSealConfigInvalid)
            | (RvError::ErrCoreSealConfigNotFound, RvError::ErrCoreSealConfigNotFound)
            | (RvError::ErrCoreRouterNotHandling, RvError::ErrCoreRouterNotHandling)
            | (RvError::ErrCoreHandlerExist, RvError::ErrCoreHandlerExist)
            | (RvError::ErrPhysicalConfigItemMissing, RvError::ErrPhysicalConfigItemMissing)
            | (RvError::ErrPhysicalTypeInvalid, RvError::ErrPhysicalTypeInvalid)
            | (
                RvError::ErrPhysicalBackendPrefixInvalid,
                RvError::ErrPhysicalBackendPrefixInvalid,
            )
            | (RvError::ErrPhysicalBackendKeyInvalid, RvError::ErrPhysicalBackendKeyInvalid)
            | (RvError::ErrBarrierKeySanityCheckFailed, RvError::ErrBarrierKeySanityCheckFailed)
            | (RvError::ErrBarrierAlreadyInit, RvError::ErrBarrierAlreadyInit)
            | (RvError::ErrBarrierKeyInvalid, RvError::ErrBarrierKeyInvalid)
            | (RvError::ErrBarrierNotInit, RvError::ErrBarrierNotInit)
            | (RvError::ErrBarrierSealed, RvError::ErrBarrierSealed)
            | (RvError::ErrBarrierUnsealed, RvError::ErrBarrierUnsealed)
            | (RvError::ErrBarrierUnsealFailed, RvError::ErrBarrierUnsealFailed)
            | (RvError::ErrBarrierEpochMismatch, RvError::ErrBarrierEpochMismatch)
            | (RvError::ErrBarrierVersionMismatch, RvError::ErrBarrierVersionMismatch)
            | (RvError::ErrBarrierKeyGenerationFailed, RvError::ErrBarrierKeyGenerationFailed)
            | (RvError::ErrRouterMountConflict, RvError::ErrRouterMountConflict)
            | (RvError::ErrRouterMountNotFound, RvError::ErrRouterMountNotFound)
            | (RvError::ErrMountFailed, RvError::ErrMountFailed)
            | (RvError::ErrMountPathProtected, RvError::ErrMountPathProtected)
            | (RvError::ErrMountPathExist, RvError::ErrMountPathExist)
            | (RvError::ErrMountTableNotFound, RvError::ErrMountTableNotFound)
            | (RvError::ErrMountTableNotReady, RvError::ErrMountTableNotReady)
            | (RvError::ErrMountNotMatch, RvError::ErrMountNotMatch)
            | (RvError::ErrLogicalPathUnsupported, RvError::ErrLogicalPathUnsupported)
            | (RvError::ErrLogicalOperationUnsupported, RvError::ErrLogicalOperationUnsupported)
            | (RvError::ErrRequestNotReady, RvError::ErrRequestNotReady)
            | (RvError::ErrRequestNoData, RvError::ErrRequestNoData)
            | (RvError::ErrRequestNoDataField, RvError::ErrRequestNoDataField)
            | (RvError::ErrRequestInvalid, RvError::ErrRequestInvalid)
            | (RvError::ErrRequestClientTokenMissing, RvError::ErrRequestClientTokenMissing)
            | (RvError::ErrRequestFieldNotFound, RvError::ErrRequestFieldNotFound)
            | (RvError::ErrRequestFieldInvalid, RvError::ErrRequestFieldInvalid)
            | (RvError::ErrResponseDataInvalid, RvError::ErrResponseDataInvalid)
            | (RvError::ErrHandlerDefault, RvError::ErrHandlerDefault)
            | (RvError::ErrModuleKvDataFieldMissing, RvError::ErrModuleKvDataFieldMissing)
            | (RvError::ErrRustDowncastFailed, RvError::ErrRustDowncastFailed)
            | (RvError::ErrShamirShareCountInvalid, RvError::ErrShamirShareCountInvalid)
            | (RvError::ErrRwLockReadPoison, RvError::ErrRwLockReadPoison)
            | (RvError::ErrRwLockWritePoison, RvError::ErrRwLockWritePoison)
            | (RvError::ErrConfigPathInvalid, RvError::ErrConfigPathInvalid)
            | (RvError::ErrConfigLoadFailed, RvError::ErrConfigLoadFailed)
            | (RvError::ErrConfigStorageNotFound, RvError::ErrConfigStorageNotFound)
            | (RvError::ErrConfigListenerNotFound, RvError::ErrConfigListenerNotFound)
            | (RvError::ErrModuleConflict, RvError::ErrModuleConflict)
            | (RvError::ErrModuleNotInit, RvError::ErrModuleNotInit)
            | (RvError::ErrModuleNotFound, RvError::ErrModuleNotFound)
            | (RvError::ErrAuthModuleDisabled, RvError::ErrAuthModuleDisabled)
            | (RvError::ErrAuthTokenNotFound, RvError::ErrAuthTokenNotFound)
            | (RvError::ErrAuthTokenIdInvalid, RvError::ErrAuthTokenIdInvalid)
            | (RvError::ErrLeaseNotFound, RvError::ErrLeaseNotFound)
            | (RvError::ErrLeaseNotRenewable, RvError::ErrLeaseNotRenewable)
            | (RvError::ErrPermissionDenied, RvError::ErrPermissionDenied)
            | (RvError::ErrPkiPemBundleInvalid, RvError::ErrPkiPemBundleInvalid)
            | (RvError::ErrPkiCertKeyMismatch, RvError::ErrPkiCertKeyMismatch)
            | (RvError::ErrPkiCertChainIncorrect, RvError::ErrPkiCertChainIncorrect)
            | (RvError::ErrPkiCertIsNotCA, RvError::ErrPkiCertIsNotCA)
            | (RvError::ErrPkiCaKeyNotFound, RvError::ErrPkiCaKeyNotFound)
            | (RvError::ErrPkiCaNotConfig, RvError::ErrPkiCaNotConfig)
            | (RvError::ErrPkiCaExtensionIncorrect, RvError::ErrPkiCaExtensionIncorrect)
            | (RvError::ErrPkiKeyTypeInvalid, RvError::ErrPkiKeyTypeInvalid)
            | (RvError::ErrPkiKeyBitsInvalid, RvError::ErrPkiKeyBitsInvalid)
            | (RvError::ErrPkiKeyNameAlreadyExist, RvError::ErrPkiKeyNameAlreadyExist)
            | (RvError::ErrPkiKeyOperationInvalid, RvError::ErrPkiKeyOperationInvalid)
            | (RvError::ErrPkiCertNotFound, RvError::ErrPkiCertNotFound)
            | (RvError::ErrPkiRoleNotFound, RvError::ErrPkiRoleNotFound)
            | (RvError::ErrPkiDataInvalid, RvError::ErrPkiDataInvalid)
            | (RvError::ErrPkiInternal, RvError::ErrPkiInternal)
            | (RvError::ErrCredentailInvalid, RvError::ErrCredentailInvalid)
            | (RvError::ErrCredentailNotConfig, RvError::ErrCredentailNotConfig)
            | (RvError::ErrUnknown, RvError::ErrUnknown) => true,
            (RvError::ErrResponse(a), RvError::ErrResponse(b)) => a == b,
            (RvError::ErrResponseStatus(sa, ta), RvError::ErrResponseStatus(sb, tb)) => {
                sa == sb && ta == tb
            }
            (RvError::ErrString(a), RvError::ErrString(b)) => a == b,
            _ => false,
        }
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for RvError {
    fn from(_: PoisonError<RwLockWriteGuard<'_, T>>) -> Self {
        RvError::ErrRwLockWritePoison
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for RvError {
    fn from(_: PoisonError<RwLockReadGuard<'_, T>>) -> Self {
        RvError::ErrRwLockReadPoison
    }
}

impl From<rustls_pemfile::Error> for RvError {
    fn from(err: rustls_pemfile::Error) -> Self {
        RvError::RustlsPemFileError(err)
    }
}

impl From<rustls::pki_types::pem::Error> for RvError {
    fn from(err: rustls::pki_types::pem::Error) -> Self {
        RvError::RustlsPkiTypesPemFileError(err)
    }
}

#[macro_export]
macro_rules! rv_error_string {
    ($message:expr) => {
        RvError::ErrString($message.to_string())
    };
}

#[macro_export]
macro_rules! rv_error_response {
    ($message:expr) => {
        RvError::ErrResponse($message.to_string())
    };
}

#[macro_export]
macro_rules! rv_error_response_status {
    ($status:expr, $message:expr) => {
        RvError::ErrResponseStatus($status, $message.to_string())
    };
}
