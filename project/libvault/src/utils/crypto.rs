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
//! The second working mode, which works as a typical Rust crate called `rusty_vault`, allows Rust
//! application developers to integrate RustyVault easily into their own applications to have the
//! ability of secrets management such as secure key/vaule storage, public key cryptography, data
//! encryption and so forth.
//!
//! This is the official documentation of crate `rusty_vault`, and it's mainly for developers.
//! Once again, if you are looking for how to use the RustyVault server via a set of RESTful API,
//! then you may prefer the RustyVault's [RESTful API documentation].
//!
//! [Hashicorp Vault]: https://www.hashicorp.com/products/vault
//! [RESTful API documentation]: https://www.tongsuo.net

use std::ops::DerefMut;

use blake2b_simd::Params;
use openssl::rand::rand_priv_bytes;
use serde::Serialize;
use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::modules::crypto::{AEADCipher, AESKeySize, BlockCipher, CipherMode, AES};

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
    #[error("Some rusty_vault error happened, {:?}", .source)]
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

        Self { key: key.to_vec(), aad: aad.to_vec() }
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

        let mut aes_decrypter =
            AES::new(false, Some(AESKeySize::AES256), Some(CipherMode::GCM), Some(self.key.clone()), Some(nonce))?;

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

        Ok(Self { ciphertext, key, _marker: std::marker::PhantomData })
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
    let hash = Params::new().hash_length(32).to_state().update(key.as_bytes()).finalize();
    hash.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: i32,
        name: String,
        value: f64,
        active: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct ComplexData {
        items: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
        timestamp: i64,
    }

    #[test]
    fn test_encrypt_decrypt_basic() {
        let original_data = TestData { id: 123, name: "test wallet".to_string(), value: 99.99, active: true };

        let encrypted_box = EncryptedBox::new(&original_data).unwrap();

        let decrypted_data = encrypted_box.get().unwrap();

        assert_eq!(original_data, decrypted_data);
    }

    #[test]
    fn test_encrypt_decrypt_complex_data() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("version".to_string(), "1.0".to_string());
        metadata.insert("type".to_string(), "wallet".to_string());

        let original_data = ComplexData {
            items: vec!["item1".to_string(), "item2".to_string(), "item3".to_string()],
            metadata,
            timestamp: 9999999999,
        };

        let encrypted_box = EncryptedBox::new(&original_data).unwrap();

        let decrypted_data = encrypted_box.get().unwrap();

        assert_eq!(original_data, decrypted_data);
    }

    #[test]
    fn test_encrypt_decrypt_empty_data() {
        let original_data = TestData { id: 0, name: "".to_string(), value: 0.0, active: false };

        let encrypted_box = EncryptedBox::new(&original_data).unwrap();
        let decrypted_data = encrypted_box.get().unwrap();

        assert_eq!(original_data, decrypted_data);
    }

    #[test]
    fn test_encrypt_decrypt_large_data() {
        let large_string = "a".repeat(10000);
        let original_data = TestData { id: 999, name: large_string, value: 123.456, active: true };

        let encrypted_box = EncryptedBox::new(&original_data).unwrap();
        let decrypted_data = encrypted_box.get().unwrap();

        assert_eq!(original_data, decrypted_data);
    }

    #[test]
    fn test_multiple_encryptions_produce_different_ciphertext() {
        let data = TestData { id: 1, name: "test".to_string(), value: 1.0, active: true };

        let encrypted1 = EncryptedBox::new(&data).unwrap();
        let encrypted2 = EncryptedBox::new(&data).unwrap();

        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        let decrypted1 = encrypted1.get().unwrap();
        let decrypted2 = encrypted2.get().unwrap();
        assert_eq!(decrypted1, decrypted2);
        assert_eq!(decrypted1, data);
    }

    #[test]
    fn test_encrypted_box_structure() {
        let data = TestData { id: 42, name: "structure test".to_string(), value: 42.0, active: false };

        let encrypted_box = EncryptedBox::new(&data).unwrap();

        assert!(!encrypted_box.ciphertext.is_empty());

        let decrypted = encrypted_box.get().unwrap();
        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let data = TestData { id: 100, name: "serialization test".to_string(), value: 100.0, active: true };

        let json = serde_json::to_string(&data).unwrap();
        let deserialized: TestData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, deserialized);

        let encrypted = EncryptedBox::new(&data).unwrap();
        let decrypted = encrypted.get().unwrap();
        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_new() {
        let key1 = CryptoKey::new();
        let key2 = CryptoKey::new();

        assert_eq!(key1.key.len(), 32);
        assert_eq!(key1.aad.len(), 16);

        assert_ne!(key1.key, key2.key);
        assert_ne!(key1.aad, key2.aad);
    }

    #[test]
    fn test_crypto_key_encrypt_decrypt_basic() {
        let key = CryptoKey::new();
        let data = TestData { id: 123, name: "test".to_string(), value: 45.67, active: true };

        let encrypted = key.encrypt(&data).unwrap();
        let decrypted: TestData = key.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_encrypt_decrypt_complex() {
        let key = CryptoKey::new();
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let data =
            ComplexData { items: vec!["item1".to_string(), "item2".to_string()], metadata, timestamp: 1234567890 };

        let encrypted = key.encrypt(&data).unwrap();
        let decrypted: ComplexData = key.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_encrypt_decrypt_empty() {
        let key = CryptoKey::new();
        let data = TestData { id: 0, name: "".to_string(), value: 0.0, active: false };

        let encrypted = key.encrypt(&data).unwrap();
        let decrypted: TestData = key.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_encrypt_decrypt_large_data() {
        let key = CryptoKey::new();
        let large_string = "x".repeat(5000);
        let data = TestData { id: 999, name: large_string, value: 999.999, active: true };

        let encrypted = key.encrypt(&data).unwrap();
        let decrypted: TestData = key.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_multiple_encryptions() {
        let key = CryptoKey::new();
        let data = TestData { id: 42, name: "multiple".to_string(), value: 42.0, active: false };

        let encrypted1 = key.encrypt(&data).unwrap();
        let encrypted2 = key.encrypt(&data).unwrap();

        assert_ne!(encrypted1, encrypted2);

        let decrypted1: TestData = key.decrypt(&encrypted1).unwrap();
        let decrypted2: TestData = key.decrypt(&encrypted2).unwrap();

        assert_eq!(decrypted1, decrypted2);
        assert_eq!(decrypted1, data);
    }

    #[test]
    fn test_crypto_key_different_keys() {
        let key1 = CryptoKey::new();
        let key2 = CryptoKey::new();
        let data = TestData { id: 100, name: "different keys".to_string(), value: 100.0, active: true };

        let encrypted1 = key1.encrypt(&data).unwrap();
        let encrypted2 = key2.encrypt(&data).unwrap();

        assert_ne!(encrypted1, encrypted2);

        let decrypted1: TestData = key1.decrypt(&encrypted1).unwrap();
        let decrypted2: TestData = key2.decrypt(&encrypted2).unwrap();

        assert_eq!(decrypted1, data);
        assert_eq!(decrypted2, data);
    }

    #[test]
    fn test_crypto_key_decrypt_wrong_key() {
        let key1 = CryptoKey::new();
        let key2 = CryptoKey::new();
        let data = TestData { id: 200, name: "wrong key".to_string(), value: 200.0, active: true };

        let encrypted = key1.encrypt(&data).unwrap();

        let result: Result<TestData, _> = key2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_crypto_key_decrypt_corrupted_data() {
        let key = CryptoKey::new();
        let data = TestData { id: 300, name: "corrupted".to_string(), value: 300.0, active: true };

        let encrypted = key.encrypt(&data).unwrap();

        let mut corrupted = encrypted.clone();
        if corrupted.len() > 20 {
            corrupted[20] = corrupted[20].wrapping_add(1);
        }

        let result: Result<TestData, _> = key.decrypt(&corrupted);
        assert!(result.is_err());
    }

    #[test]
    fn test_crypto_key_decrypt_too_short_data() {
        let key = CryptoKey::new();
        let short_data = vec![1, 2, 3, 4, 5]; // 太短的数据

        let result: Result<TestData, _> = key.decrypt(&short_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_crypto_key_decrypt_empty_data() {
        let key = CryptoKey::new();
        let empty_data = vec![];

        let result: Result<TestData, _> = key.decrypt(&empty_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_crypto_key_serialization() {
        let key = CryptoKey::new();
        let data = TestData { id: 400, name: "serialization".to_string(), value: 400.0, active: true };

        let encrypted = key.encrypt(&data).unwrap();

        let serialized = serde_json::to_string(&key).unwrap();
        let deserialized: CryptoKey = serde_json::from_str(&serialized).unwrap();

        let decrypted: TestData = deserialized.decrypt(&encrypted).unwrap();
        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_edge_cases() {
        let key = CryptoKey::new();

        let test_cases = vec![
            TestData { id: i32::MIN, name: "min".to_string(), value: f64::MIN, active: false },
            TestData { id: i32::MAX, name: "max".to_string(), value: f64::MAX, active: true },
            TestData { id: 0, name: "".to_string(), value: 0.0, active: false },
            TestData { id: -1, name: "negative".to_string(), value: -1.0, active: true },
        ];

        for test_case in test_cases {
            let encrypted = key.encrypt(&test_case).unwrap();
            let decrypted: TestData = key.decrypt(&encrypted).unwrap();
            assert_eq!(test_case, decrypted);
        }
    }

    #[test]
    fn test_crypto_key_nested_structures() {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct NestedData {
            inner: TestData,
            count: usize,
            optional: Option<String>,
        }

        let key = CryptoKey::new();
        let data = NestedData {
            inner: TestData { id: 1, name: "nested".to_string(), value: 1.0, active: true },
            count: 42,
            optional: Some("optional".to_string()),
        };

        let encrypted = key.encrypt(&data).unwrap();
        let decrypted: NestedData = key.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_crypto_key_performance() {
        let key = CryptoKey::new();
        let data = TestData { id: 600, name: "performance".to_string(), value: 600.0, active: true };

        for _ in 0..100 {
            let encrypted = key.encrypt(&data).unwrap();
            let decrypted: TestData = key.decrypt(&encrypted).unwrap();
            assert_eq!(data, decrypted);
        }
    }
}
