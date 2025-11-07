//! This is the OpenSSL adaptor.

use openssl::{
    rand::rand_priv_bytes,
    symm::{Cipher, Crypter, Mode, decrypt, decrypt_aead, encrypt, encrypt_aead},
};

use crate::{
    errors::RvError,
    modules::crypto::{
        AEADCipher, AES, AESKeySize, BlockCipher, CipherMode, crypto_adaptors::common,
    },
};

pub struct AdaptorCTX {
    ctx: Crypter,
    tag_set: bool,
    aad_set: bool,
}

impl AES {
    /// This function is the constructor of the AES struct, it returns a new AES object on success.
    ///
    /// keygen: true stands for generating a key and iv; if false, then the caller needs to feed in
    /// the specific key and iv values through the parameters.
    /// size: bit-length of AES. If omitted, AESKeySize::AES128 is used as default.
    /// mode: cipher mode of AES, such as CBC, GCM, etc. If omitted, CipherMode::CBC is default.
    /// key: symmetric key that is used to encrypt and decrypt data.
    /// iv: initialization vector. This depends on specific mode, for instance, ECB requires no IV.
    pub fn new(
        keygen: bool,
        size: Option<AESKeySize>,
        mode: Option<CipherMode>,
        key: Option<Vec<u8>>,
        iv: Option<Vec<u8>>,
    ) -> Result<Self, RvError> {
        common_aes_new!(keygen, size, mode, key, iv);
    }

    /// This function returns the key and iv vaule stored in one AES object.
    ///
    /// Two values are returned in a tuple: the first element represents the key, and the second
    /// element represents the IV. Elements may be None if unset.
    pub fn get_key_iv(&self) -> (Vec<u8>, Vec<u8>) {
        common_get_key_iv!(self);
    }
}

impl BlockCipher for AES {
    fn encrypt(&mut self, plaintext: &Vec<u8>) -> Result<Vec<u8>, RvError> {
        common_aes_encrypt!(self, plaintext);
    }

    fn encrypt_update(
        &mut self,
        plaintext: Vec<u8>,
        ciphertext: &mut Vec<u8>,
    ) -> Result<usize, RvError> {
        common_aes_encrypt_update!(self, plaintext, ciphertext);
    }

    fn encrypt_final(&mut self, ciphertext: &mut Vec<u8>) -> Result<usize, RvError> {
        common_aes_encrypt_final!(self, ciphertext);
    }

    fn decrypt(&mut self, ciphertext: &Vec<u8>) -> Result<Vec<u8>, RvError> {
        common_aes_decrypt!(self, ciphertext);
    }

    fn decrypt_update(
        &mut self,
        ciphertext: Vec<u8>,
        plaintext: &mut Vec<u8>,
    ) -> Result<usize, RvError> {
        common_aes_decrypt_update!(self, ciphertext, plaintext);
    }

    fn decrypt_final(&mut self, plaintext: &mut Vec<u8>) -> Result<usize, RvError> {
        common_aes_decrypt_final!(self, plaintext);
    }
}

impl AEADCipher for AES {
    fn set_aad(&mut self, aad: Vec<u8>) -> Result<(), RvError> {
        common_aes_set_aad!(self, aad);
    }
    fn get_tag(&mut self) -> Result<Vec<u8>, RvError> {
        common_aes_get_tag!(self);
    }
    fn set_tag(&mut self, tag: Vec<u8>) -> Result<(), RvError> {
        common_aes_set_tag!(self, tag);
    }
}
