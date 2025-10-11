//! The libvault::crypto module abstracts a set of generic cryptography methods. These methods
//! are used by other modules in RustyVault.
//!
//! This module depends on underlying cryptography library. One crypto adaptors MUST be specified
//! during the configuration, then it's compiled into RustyVault.
//!
//! # Crypto Adaptor Configurations
//!
//! In RustyVault, the real cryptographic operations are done via "crypto_adaptor"s.
//!
//! A crypto adaptor is a module that conveys and translates high level cryptography
//! operations like encryption, signing into the APIs provided by underlying cryptography
//! libraries such as OpenSSL, Tongsuo and so forth.
//!
//! At current stage, only one crypto_adaptor can be enabled at compilation phase and later
//! be used at run-time. "crypto_adaptor"s are configured as 'feature's in the Cargo context.
//!
//! Currently, the supported feature names of crypto adaptors are as follows, you can enable
//! them by adding one '--features crypto_adaptor_name' option when running "cargo build":
//!
//! 1. the OpenSSL adaptor: crypto_adaptor_openssl
//! 2. the Tongsuo adaptor: crypto_adaptor_tongsuo
//!
//! If there is no explicit crypto adpator configured, then the `crypto_adaptor_openssl` is used as
//! the default option.
//!
//! # Enable the Tongsuo adaptor
//!
//! Tongsuo is a variant of OpenSSL but with more features on SMx algorithms and protocols.
//! RustyVault can use SM algorithms only if Tongsuo is built as the crypto adaptor.
//!
//! You need to build and install Tongsuo first into your local environment before building
//! RustyVault with Tongsuo. Check the following link for detailed installation steps:
//! [Tongsuo](https://github.com/Tongsuo-Project/Tongsuo)
//!
//! ~~~text
//! $ export OPENSSL_DIR=/path/to/tongsuo/install/directory
//! $ cargo build --features crypto_adaptor_tongsuo \
//!    --no-default-features \
//!    --config 'patch.crates-io.openssl.git="https://github.com/Tongsuo-Project/rust-tongsuo.git"'\
//!    --config 'patch.crates-io.openssl-sys.git="https://github.com/Tongsuo-Project/rust-tongsuo.git"'
//! ~~~
//!
//! Or you can just uncomment the following lines in Cargo.toml:
//!
//! ~~~text
//! #[patch.crates-io]
//! #openssl = { git = "https://github.com/Tongsuo-Project/rust-tongsuo.git" }
//! #openssl-sys = { git = "https://github.com/Tongsuo-Project/rust-tongsuo.git" }
//! ~~~
//!
//! and then:
//!
//! ~~~text
//! $ cargo build --features crypto_adaptor_tongsuo --no-default-features
//! ~~~

#[cfg(feature = "crypto_adaptor_openssl")]
use crypto_adaptors::openssl_adaptor::AdaptorCTX;
#[cfg(feature = "crypto_adaptor_tongsuo")]
use crypto_adaptors::tongsuo_adaptor::AdaptorCTX;

use crate::errors::RvError;

pub mod crypto_adaptors;

/// This defines common modes for block ciphers.
#[derive(PartialEq)]
pub enum CipherMode {
    CBC,
    GCM,
    CCM,
}

/// This enum defines common AES key size constants.
pub enum AESKeySize {
    AES128,
    AES192,
    AES256,
}

/// This enum defines public key algorithm type constants.
pub enum PublicKeyType {
    RSA,
    ECDSA,
    SM2,
}

// All structs are defined here. Every struct represents a type of cryptography algorithm.

/// The AES block cipher structure.
// we add this lint here because it's not guaranteed all underlying adaptors support this
// algorithm. And it's logical to suppress warnings when building the code.
#[allow(dead_code)]
pub struct AES {
    alg: (AESKeySize, CipherMode),
    key: Vec<u8>,
    iv: Vec<u8>,
    aad: Option<Vec<u8>>,
    tag: Option<Vec<u8>>,
    ctx: Option<AdaptorCTX>,
}

/// The SM4 block cipher structure.
#[allow(dead_code)]
pub struct SM4 {
    mode: CipherMode,
    key: Vec<u8>,
    iv: Vec<u8>,
    aad: Option<Vec<u8>>,
    tag: Option<Vec<u8>>,
    ctx: Option<AdaptorCTX>,
}

/// BlockCipher is the 'base' trait for all kinds of block cipher alogrithms. In this trait,
/// neccessary methods are defined. Cryptography adaptors need to implement this trait to provide
/// real algorithms.
///
/// # Examples
///
/// The following are some examples on how to use the functions of trait BlockCipher.
///
/// ## One-shot encryption and decryption
///
/// ~~~
/// use libvault::modules::crypto::{AES, AESKeySize, CipherMode, BlockCipher};
///
/// let data = b"The best way to not feel hopeless is to get up and do something.".to_vec();
/// let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F".to_vec();
/// let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x00\x01\x02\x03\x04\x05\x06\x07".to_vec();
/// let mut aes_encrypter = AES::new(false, Some(AESKeySize::AES128),
///     Some(CipherMode::CBC), Some(key.clone()), Some(iv.clone())).unwrap();
/// let mut aes_decrypter = AES::new(false, Some(AESKeySize::AES128),
///     Some(CipherMode::CBC), Some(key), Some(iv)).unwrap();
///
/// let ct = aes_encrypter.encrypt(&data).unwrap();
/// let pt = aes_decrypter.decrypt(&ct).unwrap();
/// assert_eq!(data.to_vec(), pt);
/// ~~~
///
/// ## Stream encryption and decryption
///
/// The following code works only with `crypto_adaptor_tongsuo`.
///
#[cfg_attr(feature = "crypto_adaptor_tongsuo", doc = "~~~")]
#[cfg_attr(not(feature = "crypto_adaptor_tongsuo"), doc = "~~~ignore")]
/// use libvault::modules::crypto::{SM4, CipherMode, BlockCipher};
///
/// let data: [&[u8]; 2] = [b"The best way to not feel hopeless ",
///                         b"is to get up and do something."];
/// let data2 = b"The best way to not feel hopeless is to get up and do something.";
/// let data_len = data.iter().fold(0, |sum, x| sum + x.len());
/// let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F".to_vec();
/// let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x00\x01\x02\x03\x04\x05\x06\x07".to_vec();
/// let mut sm4_encrypter = SM4::new(false, Some(CipherMode::CBC),
///     Some(key.clone()), Some(iv.clone())).unwrap();
/// let mut sm4_decrypter = SM4::new(false, Some(CipherMode::CBC),
///     Some(key), Some(iv)).unwrap();
/// let mut ct: Vec<u8> = vec![];
///
/// let mut v1: Vec<u8> = vec![0; data_len + 16];
/// let mut v2: Vec<u8>= vec![0; data_len + 16];
/// let mut v3: Vec<u8>= vec![0; data_len + 16];
/// let mut count = sm4_encrypter.encrypt_update((&data[0]).to_vec(), &mut v1).unwrap();
/// v1.truncate(count);
/// count = sm4_encrypter.encrypt_update((&data[1]).to_vec(), &mut v2).unwrap();
/// v2.truncate(count);
/// count = sm4_encrypter.encrypt_final(&mut v3).unwrap();
/// v3.truncate(count);
/// ct.extend(v1);
/// ct.extend(v2);
/// ct.extend(v3);
///
/// let data_len2 = ct.len();
/// let mut pt1: Vec<u8> = vec![0; data_len2 + 16];
/// let mut pt2: Vec<u8>= vec![0; data_len2 + 16];
/// let mut pt3: Vec<u8>= vec![0; data_len2 + 16];
/// let mut pt: Vec<u8> = vec![];
/// // separate ciphertext into 2 pieces.
/// let cts = [&ct[..9], &ct[9..]];
///
/// count = sm4_decrypter.decrypt_update((&cts[0]).to_vec(), &mut pt1).unwrap();
/// pt1.truncate(count);
/// count = sm4_decrypter.decrypt_update((&cts[1]).to_vec(), &mut pt2).unwrap();
/// pt2.truncate(count);
/// count = sm4_decrypter.decrypt_final(&mut pt3).unwrap();
/// pt3.truncate(count);
/// pt.extend(pt1);
/// pt.extend(pt2);
/// pt.extend(pt3);
///
/// // evaluate the result.
/// assert_eq!(data2.to_vec(), pt);
/// ~~~
///
/// ## Use an auto-generated key
///
/// ~~~
/// use libvault::modules::crypto::{AES, AESKeySize, CipherMode, BlockCipher};
///
/// let data = b"The best way to not feel hopeless is to get up and do something.".to_vec();
/// let mut aes_encrypter = AES::new(true, Some(AESKeySize::AES128),
///     Some(CipherMode::CBC), None, None).unwrap();
/// let mut aes_decrypter = AES::new(false, Some(AESKeySize::AES128),
///     Some(CipherMode::CBC), Some(aes_encrypter.get_key_iv().0),
///     Some(aes_encrypter.get_key_iv().1)).unwrap();
///
/// let ct = aes_encrypter.encrypt(&data).unwrap();
/// let pt = aes_decrypter.decrypt(&ct).unwrap();
/// assert_eq!(data, pt);
/// ~~~
pub trait BlockCipher {
    /// One-shot encryption.
    ///
    /// This function performs a "one-shot' style encryption. The data to be encrypted is fed by
    /// the `plaintext` parameter, while the ciphertext is returned in another `Vec<u8>`.
    fn encrypt(&mut self, plaintext: &Vec<u8>) -> Result<Vec<u8>, RvError>;

    /// Stream encryption - update phase.
    ///
    /// The ciphertext (encrypted data) is returned via the `ciphertext` parameter. The bytes that
    /// has been encrypted is returned in the return value of this function.
    ///
    /// Plaintext is fed by the `plaintext` parameter.
    fn encrypt_update(
        &mut self,
        plaintext: Vec<u8>,
        ciphertext: &mut Vec<u8>,
    ) -> Result<usize, RvError>;

    /// Stream encryption - final phase.
    ///
    /// This function finishes the encryption. Residual ciphertext is returned in the `ciphertext`
    /// parameter. `encrypt_update()` function should not be called after this function calling.
    fn encrypt_final(&mut self, ciphertext: &mut Vec<u8>) -> Result<usize, RvError>;

    /// One-shot decryption.
    ///
    /// This function performs a "one-shot' style decryption. The data to be decrypted is fed by
    /// the `ciphertext` parameter, while the plaintext is returned in another `Vec<u8>`.
    fn decrypt(&mut self, ciphertext: &Vec<u8>) -> Result<Vec<u8>, RvError>;

    /// Stream decryption - update phase.
    ///
    /// The plaintext (decrypted data) is returned via the `plaintext` parameter. The bytes that
    /// has been decrypted is returned in the return value of this function.
    ///
    /// Ciphertext is fed by the `ciphertext` parameter.
    fn decrypt_update(
        &mut self,
        ciphertext: Vec<u8>,
        plaintext: &mut Vec<u8>,
    ) -> Result<usize, RvError>;

    /// Stream decryption - final phase.
    ///
    /// This function finishes the decryption. Residual plaintext is returned in the `plaintext`
    /// parameter. `decrypt_update()` function should not be called after this function calling.
    fn decrypt_final(&mut self, plaintext: &mut Vec<u8>) -> Result<usize, RvError>;
}

/// AEADCipher defines a block cipher in AEAD mode, such as GCM or CCM.
/// This trait is an extention of BlockCipher for some additional functions.
///
/// # Examples
///
/// The following are some examples on how to use the functions of trait AEADCipher.
///
/// # One-shot encryption and decryption using AEAD cipher
///
/// ~~~
/// use libvault::modules::crypto::{AES, AESKeySize, CipherMode, BlockCipher, AEADCipher};
///
/// let data = b"The best way to not feel hopeless is to get up and do something.".to_vec();
/// let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F".to_vec();
/// let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x00\x01\x02\x03\x04\x05\x06\x07".to_vec();
/// let aad = b"some additional authenticated data.".to_vec();
/// let mut aes_encrypter = AES::new(false, Some(AESKeySize::AES128),
///     Some(CipherMode::GCM), Some(key.clone()), Some(iv.clone())).unwrap();
/// let mut aes_decrypter = AES::new(false, Some(AESKeySize::AES128),
///     Some(CipherMode::GCM), Some(key), Some(iv)).unwrap();
///
/// // set aad, encrypt and get tag.
/// aes_encrypter.set_aad(aad.clone()).unwrap();
/// let ct = aes_encrypter.encrypt(&data).unwrap();
/// let tag = aes_encrypter.get_tag().unwrap();
///
/// // set aad, set tag and decrypt.
/// aes_decrypter.set_aad(aad).unwrap();
/// aes_decrypter.set_tag(tag).unwrap();
/// let pt = aes_decrypter.decrypt(&ct).unwrap();
///
/// // evaluate the result.
/// assert_eq!(data.to_vec(), pt);
/// ~~~
///
/// # Stream encryption and decryption using AEAD cipher
///
/// The following code works only with `crypto_adaptor_tongsuo`.
///
#[cfg_attr(feature = "crypto_adaptor_tongsuo", doc = "~~~")]
#[cfg_attr(not(feature = "crypto_adaptor_tongsuo"), doc = "~~~ignore")]
/// ~~~
/// use libvault::modules::crypto::{SM4, CipherMode, BlockCipher, AEADCipher};
///
/// let data: [&[u8]; 2] = [b"The best way to not feel hopeless ",
///                         b"is to get up and do something."];
/// let data2 = b"The best way to not feel hopeless is to get up and do something.";
/// let data_len = data.iter().fold(0, |sum, x| sum + x.len());
/// let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F".to_vec();
/// let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x00\x01\x02\x03\x04\x05\x06\x07".to_vec();
/// let aad = b"some additional authenticated data.".to_vec();
/// let mut sm4_encrypter = SM4::new(false, Some(CipherMode::GCM),
///     Some(key.clone()), Some(iv.clone())).unwrap();
/// let mut sm4_decrypter = SM4::new(false, Some(CipherMode::GCM),
///     Some(key), Some(iv)).unwrap();
/// let mut ct: Vec<u8> = vec![];
///
/// // set aad, encrypt and get tag.
/// sm4_encrypter.set_aad(aad.clone()).unwrap();
/// let mut v1: Vec<u8> = vec![0; data_len + 16];
/// let mut v2: Vec<u8>= vec![0; data_len + 16];
/// let mut v3: Vec<u8>= vec![0; data_len + 16];
/// let mut count = sm4_encrypter.encrypt_update((&data[0]).to_vec(), &mut v1).unwrap();
/// v1.truncate(count);
/// count = sm4_encrypter.encrypt_update((&data[1]).to_vec(), &mut v2).unwrap();
/// v2.truncate(count);
/// count = sm4_encrypter.encrypt_final(&mut v3).unwrap();
/// v3.truncate(count);
/// ct.extend(v1);
/// ct.extend(v2);
/// ct.extend(v3);
/// let tag = sm4_encrypter.get_tag().unwrap();
///
/// // set aad, set tag and decrypt.
/// sm4_decrypter.set_aad(aad).unwrap();
/// sm4_decrypter.set_tag(tag).unwrap();
/// // separate cipher into 2 pieces.
/// let data_len2 = ct.len();
/// let mut pt1: Vec<u8> = vec![0; data_len2 + 16];
/// let mut pt2: Vec<u8>= vec![0; data_len2 + 16];
/// let mut pt3: Vec<u8>= vec![0; data_len2 + 16];
/// let mut pt: Vec<u8> = vec![];
/// let cts = [&ct[..9], &ct[9..]];
/// count = sm4_decrypter.decrypt_update((&cts[0]).to_vec(), &mut pt1).unwrap();
/// pt1.truncate(count);
/// count = sm4_decrypter.decrypt_update((&cts[1]).to_vec(), &mut pt2).unwrap();
/// pt2.truncate(count);
/// count = sm4_decrypter.decrypt_final(&mut pt3).unwrap();
/// pt3.truncate(count);
/// pt.extend(pt1);
/// pt.extend(pt2);
/// pt.extend(pt3);
///
/// // evaluate the result.
/// assert_eq!(data2.to_vec(), pt);
/// ~~~
pub trait AEADCipher: BlockCipher {
    /// Set additional authenticated data (AAD) into AEAD cipher.
    ///
    /// This must be set both at encryption and decryption. This function must be called before the
    /// updates functions like `encrypt_update()` and `decrypt_update()`.
    fn set_aad(&mut self, aad: Vec<u8>) -> Result<(), RvError>;

    /// Get the authentication tag in AEAD ciphers.
    ///
    /// This must be called after `encrypt_final()`. Tag value is returned in a `Vec<u8>`.
    fn get_tag(&mut self) -> Result<Vec<u8>, RvError>;

    /// Set the authentication tag to authenticate the ciphertext in AEAD decryption procedure.
    ///
    /// This function must be called before the `decrypt_final()` function.
    fn set_tag(&mut self, tag: Vec<u8>) -> Result<(), RvError>;
}

/// The PublicKey trait abstracts a common function set for public key algorithms. Public key
/// algorithms usually refer to signature or encryption algorithms such as RSA, SM2 and so forth.
pub trait PublicKey {
    /// Generate a pair of public and private key, based on specific algorithm type.
    fn keygen() -> Result<(), RvError>;
}

/// The Signature trait defines a signature algorithm, such as RSA, ECDSA or SM2.
/// This trait is a sub-trait of PublicKey trait.
pub trait Signature: PublicKey {
    /// Sign a piece of data and returns the generated signature value.
    ///
    /// This operation uses the private key of a specific algorithm.
    fn sign(&mut self, data: &Vec<u8>) -> Result<Vec<u8>, RvError>;

    /// Verify a piece of data against a signature and returns the verification result.
    ///
    /// This operation uses the public key of a specific algorithm.
    fn verify(&mut self, data: &Vec<u8>, sig: &Vec<u8>) -> Result<bool, RvError>;
}

/// The Encryption trait defines an public key encryption algorithm, such as RSA and SM4.
/// This trait is a sub-trait of PublicKey trait.
pub trait Encryption: PublicKey {
    /// Encrypt a piece of data using the private key.
    ///
    /// The ciphertext is returned on success.
    fn encrypt(&mut self, plaintext: &Vec<u8>) -> Result<Vec<u8>, RvError>;

    /// Decrypt a piece of data using the public key.
    ///
    /// The plaintext is returned on success.
    fn decrypt(&mut self, ciphertext: &Vec<u8>) -> Result<Vec<u8>, RvError>;
}
