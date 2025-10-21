#![allow(unused_assignments)] // zeroize-generated drop impls write fields without reading them

//! This crate is the 'library' part of RustyVault, a Rust and real free replica of Hashicorp Vault.
//! RustyVault is focused on identity-based secrets management and works in two ways independently:
//!
//! 1. A standalone application serving secrets management via RESTful API;
//! 2. A Rust crate that provides same features for other application to integrate.
//!
//! This document is only about the crate part of RustyVault. For the first working mode,
//! please go to RustyVault's [RESTful API documentation], which documents all RustyVault's RESTful API.
//! Users can use an HTTP client tool (curl, e.g.) to send commands to a running RustyVault server and
//! then have relevant secret management features.
//!
//! The second working mode, which works as a typical Rust crate called `libvault`, allows Rust
//! application developers to integrate RustyVault easily into their own applications to have the
//! ability of secrets management such as secure key/vaule storage, public key cryptography, data
//! encryption and so forth.
//!
//! This is the official documentation of crate `libvault`, and it's mainly for developers.
//! Once again, if you are looking for how to use the RustyVault server via a set of RESTful API,
//! then you may prefer the RustyVault's [RESTful API documentation].
//!
//! [Hashicorp Vault]: https://www.hashicorp.com/products/vault
//! [RESTful API documentation]: https://www.tongsuo.net

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    modules::crypto::{AEADCipher, AES, AESKeySize, BlockCipher, CipherMode},
    shamir::ShamirSecret,
    utils::BHashSet,
};

/// Error types that can occur during SealBox operations.
///
/// This enum provides a unified error type for all SealBox operations,
/// including creation, sealing, unsealing, and data access operations.
#[derive(Debug, Error)]
pub enum SealBoxError {
    /// The SealBox is currently sealed and data access is not allowed.
    ///
    /// This error occurs when trying to access data from a sealed SealBox.
    /// The SealBox must be unsealed with sufficient shares before data can be accessed.
    #[error("SealBox is sealed")]
    Sealed,

    /// The SealBox is not sealed when it should be.
    ///
    /// This error occurs when trying to perform operations that require
    /// the SealBox to be in a sealed state, but it's currently unsealed.
    #[error("SealBox is not sealed")]
    NotSealed,

    /// The SealBox is in the process of being unsealed but doesn't have enough shares yet.
    ///
    /// This error occurs when providing shares for unsealing, but the threshold
    /// number of shares hasn't been reached yet. Continue providing shares until
    /// the threshold is met.
    #[error("SealBox is unsealing")]
    Unsealing,

    /// The decryption operation failed.
    ///
    /// This error occurs when the AES-GCM decryption process fails, typically
    /// due to corrupted ciphertext, invalid authentication tag, or incorrect key.
    #[error("Decryption failed")]
    DecryptionFailed,

    /// The unsealing operation failed due to insufficient or invalid shares.
    ///
    /// This error occurs when the Shamir secret sharing reconstruction fails,
    /// typically due to insufficient shares, invalid shares, or corrupted share data.
    #[error("Unsealing failed: insufficient or invalid shares")]
    UnsealFailed,

    /// The unsealing operation failed due to a deprecated share.
    ///
    /// This error occurs when the provided share has already been used to unseal the box.
    #[error("Unsealing failed: deprecated share")]
    UnsealKeyDeprecated,

    /// The encryption operation failed.
    ///
    /// This error occurs when the AES-GCM encryption process fails, typically
    /// due to issues with key generation, nonce generation, or encryption parameters.
    #[error("Encryption failed")]
    EncryptionFailed,

    /// The Shamir secret splitting operation failed.
    ///
    /// This error occurs when creating shares from the master key fails,
    /// typically due to invalid threshold or total shares parameters.
    #[error("Shamir secret split failed")]
    ShamirSecretSplitFailed,

    /// The Shamir secret combining operation failed.
    ///
    /// This error occurs when reconstructing the master key from shares fails,
    /// typically due to insufficient shares or corrupted share data.
    #[error("Shamir secret combine failed")]
    ShamirSecretCombineFailed,
}

/// A secure container that encrypts data and distributes the decryption key using Shamir's Secret Sharing.
///
/// SealBox provides a secure way to store sensitive data by encrypting it with AES-256-GCM
/// and then splitting the encryption key using Shamir's Secret Sharing scheme. This allows
/// the data to be securely distributed among multiple parties, requiring a threshold number
/// of shares to reconstruct the key and access the data.
///
/// # Type Parameters
/// - `T`: The type of data to be stored. Must implement `Serialize` and `Deserialize`.
///
/// # Security Features
/// - Uses AES-256-GCM for authenticated encryption
/// - Implements Shamir's Secret Sharing for key distribution
/// - Automatic zeroization of sensitive data on drop
/// - Supports serialization/deserialization for persistence
/// - Configurable threshold and total shares for flexible access control
///
/// # Usage
/// 1. Create a SealBox with data, threshold, and total shares
/// 2. Distribute the shares to different parties
/// 3. To access data, collect threshold number of shares
/// 4. Use the shares to unseal the box and access the data
/// 5. Re-seal the box when done to secure the data again
#[derive(Default, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct SealBox<T> {
    /// The encrypted data (ciphertext)
    sealed_data: Vec<u8>,
    /// The nonce used for AES-GCM encryption (16 bytes)
    nonce: [u8; 16],
    /// Additional Authenticated Data (AAD) for GCM mode (13 bytes)
    aad: [u8; 13],
    /// The authentication tag from AES-GCM (16 bytes)
    tag: [u8; 16],
    /// The minimum number of shares required to unseal the box
    threshold: u8,
    /// The total number of shares to generate
    total_shares: u8,
    /// The Shamir shares for reconstructing the encryption key
    ///
    /// This field is skipped during serialization for security reasons.
    /// Shares are only stored in memory and not persisted.
    #[serde(skip)]
    shares: Option<Vec<Vec<u8>>>,
    /// The reconstructed encryption key (32 bytes for AES-256)
    ///
    /// This field is skipped during serialization for security reasons.
    /// The key is only stored in memory when the box is unsealed.
    #[serde(skip)]
    key: Option<[u8; 32]>,
    /// The decrypted and deserialized data
    ///
    /// This field is skipped during serialization and zeroization.
    /// The value is only stored in memory when the box is unsealed.
    #[serde(skip)]
    #[zeroize(skip)]
    value: Option<T>,

    /// The set of deprecated shares
    #[zeroize(skip)]
    deprecated_shares: BHashSet,
}

impl<T> SealBox<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Creates a new SealBox with the given data, threshold, and total shares.
    ///
    /// This method encrypts the provided data using AES-256-GCM and then splits
    /// the encryption key using Shamir's Secret Sharing scheme. The data is immediately
    /// available after creation, but the box can be sealed to secure the data.
    ///
    /// # Arguments
    /// - `data`: The data to be encrypted and stored
    /// - `threshold`: The minimum number of shares required to unseal the box (must be >= 2)
    /// - `total_shares`: The total number of shares to generate (must be >= threshold)
    ///
    /// # Returns
    /// A `Result` containing the new SealBox, or an error if creation fails.
    ///
    /// # Errors
    /// - Returns `SealBoxError::ShamirSecretSplitFailed` if threshold < 2 or total_shares < threshold
    /// - Returns `SealBoxError::EncryptionFailed` if the encryption process fails
    ///
    /// # Security
    /// - Uses AES-256-GCM for authenticated encryption
    /// - Generates cryptographically secure random key and nonce
    /// - Uses current timestamp as additional authenticated data
    /// - Implements Shamir's Secret Sharing for key distribution
    pub fn new(data: T, threshold: u8, total_shares: u8) -> Result<Self, SealBoxError> {
        if threshold < 2 || total_shares < threshold {
            return Err(SealBoxError::ShamirSecretSplitFailed);
        }

        let serialized = serde_json::to_vec(&data).unwrap();

        let now_ms = Utc::now()
            .timestamp_millis()
            .to_string()
            .as_bytes()
            .to_vec();

        let mut aes_encrypter = AES::new(
            true,
            Some(AESKeySize::AES256),
            Some(CipherMode::GCM),
            None,
            None,
        )
        .map_err(|_| SealBoxError::EncryptionFailed)?;

        aes_encrypter
            .set_aad(now_ms.clone())
            .map_err(|_| SealBoxError::EncryptionFailed)?;
        let encrypted = aes_encrypter
            .encrypt(&serialized)
            .map_err(|_| SealBoxError::EncryptionFailed)?;

        let mut tag: [u8; 16] = [0; 16];
        tag[..16].copy_from_slice(
            &aes_encrypter
                .get_tag()
                .map_err(|_| SealBoxError::EncryptionFailed)?,
        );

        let mut key: [u8; 32] = [0; 32];
        key[..32].copy_from_slice(&aes_encrypter.get_key_iv().0);

        let mut nonce: [u8; 16] = [0; 16];
        nonce[..16].copy_from_slice(&aes_encrypter.get_key_iv().1);

        let mut aad: [u8; 13] = [0; 13];
        aad[..13].copy_from_slice(&now_ms);

        Ok(Self {
            sealed_data: encrypted,
            nonce,
            aad,
            tag,
            threshold,
            total_shares,
            shares: None,
            key: Some(key),
            value: Some(data),
            deprecated_shares: BHashSet::default(),
        })
    }

    /// Generates Shamir secret shares for the encryption key.
    ///
    /// This method creates a set of cryptographic shares using Shamir's Secret Sharing
    /// scheme from the current encryption key. The generated shares can be distributed
    /// to multiple parties, and a threshold number of shares will be required to
    /// reconstruct the original key and unseal the data.
    ///
    /// # Returns
    /// A `Result` containing a zeroizing vector of key shares, or an error if generation fails.
    ///
    /// # Security Features
    /// - Uses the current encryption key as the source secret for sharing
    /// - Applies configured threshold and total share count from SealBox creation
    /// - Returns zeroizing vector to ensure secure memory cleanup of shares
    /// - Each share is cryptographically independent and secure
    ///
    /// # Requirements
    /// - The SealBox must be in an unsealed state (key available)
    /// - Valid threshold and total_shares configuration must exist
    ///
    /// # Usage
    /// This method is typically called after creating a new SealBox to distribute
    /// the key shares among multiple parties for secure key management. The shares
    /// can later be used with the `unseal()` method to reconstruct the key.
    pub fn generate_shares(&self) -> Result<Zeroizing<Vec<Vec<u8>>>, SealBoxError> {
        if !self.is_unsealed() {
            return Err(SealBoxError::Sealed);
        }

        let key = self.key.as_ref().ok_or(SealBoxError::Sealed)?;

        let shares = ShamirSecret::split(key, self.total_shares, self.threshold)
            .map_err(|_| SealBoxError::ShamirSecretSplitFailed)?;

        Ok(shares)
    }

    /// Internal method that performs the core unsealing logic.
    ///
    /// This private method handles the actual unsealing process by collecting shares,
    /// attempting to reconstruct the encryption key using Shamir's Secret Sharing,
    /// and decrypting the sealed data. It includes validation for deprecated shares
    /// and performs the cryptographic operations needed to recover the original data.
    ///
    /// # Arguments
    /// - `unseal_key`: A share to add to the collection for unsealing
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the unsealing operation.
    ///
    /// # Errors
    /// - Returns `SealBoxError::NotSealed` if the box is already unsealed
    /// - Returns `SealBoxError::UnsealKeyDeprecated` if the share has been used before
    /// - Returns `SealBoxError::Unsealing` if not enough shares have been provided yet
    /// - Returns `SealBoxError::UnsealFailed` if the shares are invalid or corrupted
    /// - Returns `SealBoxError::DecryptionFailed` if decryption fails
    ///
    /// # Security Features
    /// - Validates shares against deprecated share set to prevent reuse
    /// - Uses Shamir's Secret Sharing to reconstruct the encryption key
    /// - Validates authentication tag to ensure data integrity
    /// - Performs secure AES-256-GCM decryption with AAD verification
    ///
    /// # Note
    /// This is an internal method used by both `unseal()` and `unseal_once()`.
    /// The caller is responsible for managing share cleanup and deprecation policies.
    fn do_unseal(&mut self, unseal_key: &[u8]) -> Result<(), SealBoxError> {
        if self.is_unsealed() {
            return Err(SealBoxError::NotSealed);
        }

        if self.deprecated_shares.contains(unseal_key) {
            return Err(SealBoxError::UnsealKeyDeprecated);
        }

        let Some(shares) = self.shares.as_mut() else {
            self.shares = Some(vec![unseal_key.to_vec()]);
            return Err(SealBoxError::Unsealing);
        };

        if shares.len() < self.threshold as usize {
            shares.push(unseal_key.to_vec());
        }

        if shares.len() < self.threshold as usize {
            return Err(SealBoxError::Unsealing);
        }

        let key = ShamirSecret::combine(shares.clone()).ok_or(SealBoxError::UnsealFailed)?;

        let mut aes_decrypter = AES::new(
            false,
            Some(AESKeySize::AES256),
            Some(CipherMode::GCM),
            Some(key.to_vec()),
            Some(self.nonce.to_vec()),
        )
        .map_err(|_| SealBoxError::DecryptionFailed)?;

        aes_decrypter
            .set_aad(self.aad.to_vec())
            .map_err(|_| SealBoxError::DecryptionFailed)?;
        aes_decrypter
            .set_tag(self.tag.to_vec())
            .map_err(|_| SealBoxError::DecryptionFailed)?;

        let decrypted = aes_decrypter
            .decrypt(&self.sealed_data)
            .map_err(|_| SealBoxError::DecryptionFailed)?;

        let value: T =
            serde_json::from_slice(&decrypted).map_err(|_| SealBoxError::DecryptionFailed)?;

        let key: [u8; 32] = key.try_into().map_err(|_| SealBoxError::UnsealFailed)?;

        self.key = Some(key);
        self.value = Some(value);
        Ok(())
    }

    /// Attempts to unseal the box using the provided share.
    ///
    /// This method adds the provided share to the collection and attempts to
    /// reconstruct the encryption key using Shamir's Secret Sharing. If enough
    /// shares are provided (equal to or greater than the threshold), the box
    /// is unsealed and the data becomes accessible. Unlike `unseal_once()`,
    /// this method allows shares to be reused in future unsealing operations.
    ///
    /// # Arguments
    /// - `unseal_key`: A share to add to the collection for unsealing
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the unsealing operation.
    ///
    /// # Errors
    /// - Returns `SealBoxError::NotSealed` if the box is already unsealed
    /// - Returns `SealBoxError::UnsealKeyDeprecated` if the share has been marked as deprecated
    /// - Returns `SealBoxError::Unsealing` if not enough shares have been provided yet
    /// - Returns `SealBoxError::UnsealFailed` if the shares are invalid or corrupted
    /// - Returns `SealBoxError::DecryptionFailed` if decryption fails
    ///
    /// # Security Features
    /// - Uses Shamir's Secret Sharing to reconstruct the encryption key
    /// - Validates authentication tag to ensure data integrity
    /// - Clears temporary shares after successful or failed unsealing (except when more shares needed)
    /// - Respects deprecated share restrictions to prevent reuse of compromised shares
    ///
    /// # Usage
    /// This is the standard unsealing method that allows shares to be reused.
    /// Call this method multiple times with different shares until the threshold
    /// is reached and the box is successfully unsealed.
    pub fn unseal(&mut self, unseal_key: &[u8]) -> Result<(), SealBoxError> {
        let ret = self.do_unseal(unseal_key);
        match ret {
            Err(SealBoxError::Unsealing) => {}
            _ => self.shares = None,
        }
        ret
    }

    /// Unseals the box once and marks all used shares as deprecated.
    ///
    /// This method performs a one-time unsealing operation that automatically marks
    /// all shares used in the unsealing process as deprecated, preventing their reuse
    /// in future operations. This provides enhanced security by ensuring that shares
    /// can only be used once, protecting against replay attacks and share compromise.
    ///
    /// # Arguments
    /// - `unseal_key`: A share to add to the collection for unsealing
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the unsealing operation.
    ///
    /// # Errors
    /// - Returns `SealBoxError::NotSealed` if the box is already unsealed
    /// - Returns `SealBoxError::UnsealKeyDeprecated` if the share has been used before
    /// - Returns `SealBoxError::Unsealing` if not enough shares have been provided yet
    /// - Returns `SealBoxError::UnsealFailed` if the shares are invalid or corrupted
    /// - Returns `SealBoxError::DecryptionFailed` if decryption fails
    ///
    /// # Security Features
    /// - Marks all used shares as deprecated after successful unsealing
    /// - Prevents replay attacks by ensuring one-time share usage
    /// - Provides forward secrecy by invalidating used shares
    /// - Uses Shamir's Secret Sharing for secure key reconstruction
    /// - Validates authentication tag to ensure data integrity
    ///
    /// # Behavior
    /// - On successful unsealing: marks all shares as deprecated and clears share collection
    /// - On failure (except insufficient shares): clears share collection
    /// - On insufficient shares: preserves shares for additional attempts, but no deprecation
    ///
    /// # Usage
    /// This method is ideal for high-security environments where shares should only
    /// be valid for a single unsealing operation. It's commonly used in automated
    /// systems or when implementing strict access control policies.
    pub fn unseal_once(&mut self, unseal_key: &[u8]) -> Result<(), SealBoxError> {
        let ret = self.do_unseal(unseal_key);
        if ret.is_ok()
            && let Some(shares) = self.shares.as_ref()
        {
            for share in shares.iter() {
                self.deprecated_shares.insert(share);
            }
        }

        match ret {
            Err(SealBoxError::Unsealing) => {}
            _ => self.shares = None,
        }

        ret
    }

    /// Seals the box, clearing all sensitive data from memory.
    ///
    /// This method clears the shares, key, and decrypted value from memory,
    /// effectively sealing the box and requiring re-unsealing to access data.
    ///
    /// # Security
    /// - Clears all sensitive data from memory
    /// - Forces re-unsealing to access data again
    /// - Helps prevent memory-based attacks
    pub fn seal(&mut self) {
        self.shares = None;
        self.key = None;
        self.value = None;
    }

    /// Checks if the box is currently unsealed.
    ///
    /// This method returns `true` if the box has been successfully unsealed
    /// and the data is accessible, `false` otherwise.
    ///
    /// # Returns
    /// `true` if the box is unsealed and data is accessible, `false` otherwise.
    pub fn is_unsealed(&self) -> bool {
        self.key.is_some() && self.value.is_some()
    }

    /// Retrieves an immutable reference to the stored data.
    ///
    /// This method returns a reference to the decrypted data if the box
    /// is unsealed, or an error if the box is sealed.
    ///
    /// # Returns
    /// A `Result` containing a reference to the data, or an error if the box is sealed.
    ///
    /// # Errors
    /// - Returns `SealBoxError::Sealed` if the box is not unsealed
    pub fn get(&self) -> Result<&T, SealBoxError> {
        self.value.as_ref().ok_or(SealBoxError::Sealed)
    }

    /// Retrieves a mutable reference to the stored data.
    ///
    /// This method returns a mutable reference to the decrypted data if the box
    /// is unsealed, allowing modification of the stored data.
    ///
    /// # Returns
    /// A `Result` containing a mutable reference to the data, or an error if the box is sealed.
    ///
    /// # Errors
    /// - Returns `SealBoxError::Sealed` if the box is not unsealed
    ///
    /// # Security
    /// - Modifications to the data are not automatically re-encrypted
    /// - Call `seal()` and `unseal()` again to persist changes
    pub fn get_mut(&mut self) -> Result<&mut T, SealBoxError> {
        self.value.as_mut().ok_or(SealBoxError::Sealed)
    }
}
