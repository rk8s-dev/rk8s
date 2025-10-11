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

use std::ops::DerefMut;

use blake2b_simd::Params;
use openssl::rand::rand_priv_bytes;
use serde::Serialize;
use serde::{Deserialize, de::DeserializeOwned};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::modules::crypto::{AEADCipher, AES, AESKeySize, BlockCipher, CipherMode};

/// Error types that can occur during cryptographic operations.
///
/// This enum provides a unified error type for all cryptographic operations
/// in the module, including encryption, decryption, serialization, and
/// other crypto-related errors.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// A custom error with a descriptive message.
    ///
    /// Used for errors that don't fit into the other categories,
    /// such as invalid input data or unsupported operations.
    #[error("Crypto error: {0}")]
    Custom(String),

    /// An error that occurred during JSON serialization or deserialization.
    ///
    /// This error is automatically converted from `serde_json::Error`
    /// and typically occurs when encrypting/decrypting data that
    /// cannot be properly serialized or deserialized.
    #[error("Some serde_json error happened, {:?}", .source)]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },

    /// An error that occurred during OpenSSL cryptographic operations.
    ///
    /// This error is automatically converted from `openssl::error::ErrorStack`
    /// and typically occurs during encryption, decryption, or key generation
    /// operations when the underlying OpenSSL library encounters an error.
    #[error("Some openssl error happened, {:?}", .source)]
    OpenSSL {
        #[from]
        source: openssl::error::ErrorStack,
    },

    /// An error that occurred in the RustyVault core system.
    ///
    /// This error is automatically converted from `crate::errors::RvError`
    /// and typically occurs when the cryptographic operation interacts
    /// with other parts of the RustyVault system.
    #[error("Some libvault error happened, {:?}", .source)]
    RvError {
        #[from]
        source: crate::errors::RvError,
    },
}

type Result<T, E = CryptoError> = std::result::Result<T, E>;

/// A cryptographic key used for encryption and decryption operations.
///
/// This struct provides a secure way to encrypt and decrypt data using AES-256-GCM.
/// The key and additional authenticated data (AAD) are automatically zeroized when dropped
/// for security purposes.
///
/// # Security Features
/// - Uses AES-256-GCM for authenticated encryption
/// - Automatically generates random nonces for each encryption
/// - Implements zeroization for secure memory cleanup
/// - Supports serialization/deserialization for persistence
#[derive(Default, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct CryptoKey {
    /// The encryption key (32 bytes for AES-256)
    key: Vec<u8>,
    /// Additional Authenticated Data (AAD) for GCM mode (16 bytes)
    aad: Vec<u8>,
}

/// A generic encrypted container that holds encrypted data along with its encryption key.
///
/// This struct provides a convenient way to store encrypted data with its associated
/// cryptographic key. The entire structure is zeroized when dropped to ensure
/// secure memory cleanup.
///
/// # Type Parameters
/// - `T`: The type of data to be encrypted/decrypted. Must implement `Serialize` and `DeserializeOwned`.
///
/// # Security Features
/// - Encapsulates both ciphertext and encryption key
/// - Automatic zeroization on drop
/// - Type-safe encryption/decryption operations
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct EncryptedBox<T> {
    /// The encrypted data (ciphertext)
    ciphertext: Vec<u8>,
    /// The cryptographic key used for encryption/decryption
    key: CryptoKey,
    /// Phantom data to maintain type information
    #[zeroize(skip)]
    _marker: std::marker::PhantomData<T>,
}

impl CryptoKey {
    /// Creates a new cryptographic key with randomly generated key and AAD.
    ///
    /// This method generates a cryptographically secure random 32-byte key
    /// and 16-byte additional authenticated data (AAD) for AES-256-GCM encryption.
    ///
    /// # Returns
    /// A new `CryptoKey` instance with randomly generated components.
    ///
    /// # Security
    /// - Uses cryptographically secure random number generation
    /// - Key and AAD are generated independently
    /// - All sensitive data is zeroized on drop
    pub fn new() -> Self {
        let mut key = Zeroizing::new(vec![0u8; 32]);
        let _ = rand_priv_bytes(key.deref_mut().as_mut_slice());

        let mut aad = Zeroizing::new(vec![0u8; 16]);
        let _ = rand_priv_bytes(aad.deref_mut().as_mut_slice());

        Self {
            key: key.to_vec(),
            aad: aad.to_vec(),
        }
    }

    /// Encrypts a serializable value using AES-256-GCM.
    ///
    /// This method serializes the input value to JSON, then encrypts it using
    /// AES-256-GCM with a randomly generated nonce. The result includes the nonce,
    /// authentication tag, and ciphertext concatenated together.
    ///
    /// # Type Parameters
    /// - `T`: The type to encrypt. Must implement `Serialize` and `DeserializeOwned`.
    ///
    /// # Arguments
    /// - `value`: The value to encrypt
    ///
    /// # Returns
    /// A `Result` containing the encrypted data as a byte vector, or an error if encryption fails.
    ///
    /// # Format
    /// The returned data has the following structure:
    /// - Bytes 0-15: Random nonce (16 bytes)
    /// - Bytes 16-31: Authentication tag (16 bytes)
    /// - Bytes 32+: Encrypted ciphertext
    ///
    /// # Security
    /// - Uses AES-256-GCM for authenticated encryption
    /// - Generates a fresh random nonce for each encryption
    /// - Includes authentication tag for integrity verification
    pub fn encrypt<T: Serialize + DeserializeOwned>(&self, value: &T) -> Result<Vec<u8>> {
        let plaintext = serde_json::to_vec(value)?;

        let mut nonce = vec![0u8; 16];
        let _ = rand_priv_bytes(&mut nonce);

        let mut aes_encrypter = AES::new(
            false,
            Some(AESKeySize::AES256),
            Some(CipherMode::GCM),
            Some(self.key.clone()),
            Some(nonce.clone()),
        )?;

        aes_encrypter.set_aad(self.aad.clone())?;

        let ciphertext = aes_encrypter.encrypt(&plaintext)?;
        let tag = aes_encrypter.get_tag()?;

        let mut result = vec![];
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&tag);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypts previously encrypted data and deserializes it to the original type.
    ///
    /// This method reverses the encryption process by extracting the nonce and tag
    /// from the encrypted data, then decrypting the ciphertext and deserializing
    /// the result back to the original type.
    ///
    /// # Type Parameters
    /// - `T`: The type to decrypt to. Must implement `Serialize` and `DeserializeOwned`.
    ///
    /// # Arguments
    /// - `value`: The encrypted data to decrypt
    ///
    /// # Returns
    /// A `Result` containing the decrypted and deserialized value, or an error if decryption fails.
    ///
    /// # Errors
    /// - Returns `CryptoError::Custom` if the input data is too short (< 32 bytes)
    /// - Returns `CryptoError::OpenSSL` if decryption fails
    /// - Returns `CryptoError::SerdeJson` if deserialization fails
    ///
    /// # Security
    /// - Verifies data integrity using the authentication tag
    /// - Uses the same AAD that was used during encryption
    /// - Validates ciphertext length before processing
    pub fn decrypt<T: Serialize + DeserializeOwned>(&self, value: &[u8]) -> Result<T> {
        if value.len() < 32 {
            return Err(CryptoError::Custom("Invalid ciphertext length".to_string()));
        }

        let nonce = value[0..16].to_vec();
        let tag = value[16..32].to_vec();

        let mut aes_decrypter = AES::new(
            false,
            Some(AESKeySize::AES256),
            Some(CipherMode::GCM),
            Some(self.key.clone()),
            Some(nonce),
        )?;

        aes_decrypter.set_aad(self.aad.clone())?;

        aes_decrypter.set_tag(tag)?;

        let plaintext = aes_decrypter.decrypt(&value[32..].to_vec())?;

        Ok(serde_json::from_slice(&plaintext)?)
    }
}

impl<T> EncryptedBox<T>
where
    T: Serialize + DeserializeOwned,
{
    /// Creates a new encrypted box containing the given value.
    ///
    /// This method creates a new `CryptoKey`, encrypts the provided value,
    /// and stores both the ciphertext and the key in the encrypted box.
    ///
    /// # Type Parameters
    /// - `T`: The type of value to encrypt. Must implement `Serialize` and `DeserializeOwned`.
    ///
    /// # Arguments
    /// - `value`: The value to encrypt and store
    ///
    /// # Returns
    /// A `Result` containing the new `EncryptedBox`, or an error if encryption fails.
    ///
    /// # Security
    /// - Generates a fresh cryptographic key for each box
    /// - Uses AES-256-GCM for authenticated encryption
    /// - All sensitive data is zeroized when the box is dropped
    pub fn new(value: &T) -> Result<Self> {
        let key = CryptoKey::new();
        let ciphertext = key.encrypt(value)?;

        Ok(Self {
            ciphertext,
            key,
            _marker: std::marker::PhantomData,
        })
    }

    /// Retrieves and decrypts the stored value.
    ///
    /// This method decrypts the stored ciphertext using the associated key
    /// and deserializes the result back to the original type.
    ///
    /// # Returns
    /// A `Result` containing the decrypted value, or an error if decryption fails.
    ///
    /// # Errors
    /// - Returns `CryptoError::Custom` if the ciphertext is invalid
    /// - Returns `CryptoError::OpenSSL` if decryption fails
    /// - Returns `CryptoError::SerdeJson` if deserialization fails
    ///
    /// # Security
    /// - Verifies data integrity using the authentication tag
    /// - Uses the same cryptographic key that was used for encryption
    /// - Validates ciphertext format before processing
    pub fn get(&self) -> Result<T> {
        let value: T = self.key.decrypt(&self.ciphertext)?;
        Ok(value)
    }
}

/// Computes a Blake2b-256 hash of the given key string.
///
/// This function uses the Blake2b hashing algorithm with a 256-bit (32-byte) output
/// to create a cryptographic hash of the input key string.
///
/// # Arguments
/// - `key`: The string to hash
///
/// # Returns
/// A 32-byte vector containing the hash digest.
///
/// # Security
/// - Uses Blake2b, a cryptographically secure hash function
/// - Produces a 256-bit (32-byte) hash output
/// - Deterministic: same input always produces same output
/// - Collision-resistant and preimage-resistant
pub fn blake2b256_hash(key: &str) -> Vec<u8> {
    let hash = Params::new()
        .hash_length(32)
        .to_state()
        .update(key.as_bytes())
        .finalize();
    hash.as_bytes().to_vec()
}
