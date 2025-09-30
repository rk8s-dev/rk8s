//! This is the Tongsuo adaptor.

use openssl::{
    rand::rand_priv_bytes,
    symm::{Cipher, Crypter, Mode, decrypt, decrypt_aead, encrypt, encrypt_aead},
};

use crate::{
    errors::RvError,
    modules::crypto::{
        AEADCipher, AES, AESKeySize, BlockCipher, CipherMode, SM4, crypto_adaptors::common,
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

impl SM4 {
    /// This function is the constructor of the SM4 struct, it returns a new SM4 object on success.
    ///
    /// keygen: true stands for generating a key and iv; if false, then the caller needs to feed in
    /// the specific key and iv values through the parameters.
    /// mode: cipher mode of SM4, such as CBC, GCM, etc. If omitted, CipherMode::CBC is default.
    /// key: symmetric key that is used to encrypt and decrypt data.
    /// iv: initialization vector. This depends on specific mode, for instance, ECB requires no IV.
    pub fn new(
        keygen: bool,
        mode: Option<CipherMode>,
        key: Option<Vec<u8>>,
        iv: Option<Vec<u8>>,
    ) -> Result<Self, RvError> {
        // default algorithm: SM4-CBC.
        let mut c_mode = CipherMode::CBC;
        let sm4_key: Vec<u8>;
        let sm4_iv: Vec<u8>;

        if let Some(x) = mode {
            c_mode = x;
        }

        if keygen == false {
            match (key, iv) {
                (Some(x), Some(y)) => {
                    sm4_key = x.clone();
                    sm4_iv = y.clone();
                }
                _ => return Err(RvError::ErrCryptoCipherInitFailed),
            }
        } else {
            // generate new key and iv based on k_size.
            // for SM4, key is 16 bytes and iv is 16 bytes
            let mut buf = [0; 16];
            let mut buf2 = [0; 16];
            rand_priv_bytes(&mut buf)?;
            sm4_key = buf.to_vec();
            rand_priv_bytes(&mut buf2)?;
            sm4_iv = buf2.to_vec();
        }

        Ok(SM4 {
            mode: c_mode,
            key: sm4_key,
            iv: sm4_iv,
            aad: None,
            ctx: None,
            tag: None,
        })
    }

    /// This function returns the key and iv vaule stored in one SM4 object.
    ///
    /// Two values are returned in a tuple: the first element represents the key, and the second
    /// element represents the IV. Elements may be None if unset.
    pub fn get_key_iv(&self) -> (Vec<u8>, Vec<u8>) {
        (self.key.clone(), self.iv.clone())
    }
}

impl BlockCipher for SM4 {
    fn encrypt(&mut self, plaintext: &Vec<u8>) -> Result<Vec<u8>, RvError> {
        match self.mode {
            CipherMode::CBC => {
                let ciphertext = encrypt(Cipher::sm4_cbc(), &self.key, Some(&self.iv), plaintext)?;
                return Ok(ciphertext.to_vec());
            }
            CipherMode::GCM => {
                // aes_128_gcm's tag is 16-bytes long.
                let tag: &mut [u8] = &mut [0; 16];
                let ciphertext = encrypt_aead(
                    Cipher::sm4_gcm(),
                    &self.key,
                    Some(&self.iv),
                    &self.aad.clone().unwrap(),
                    plaintext,
                    tag,
                )?;
                self.tag = Some(tag.to_vec());
                return Ok(ciphertext.to_vec());
            }
            _ => Err(RvError::ErrCryptoCipherOPNotSupported),
        }
    }

    fn encrypt_update(
        &mut self,
        plaintext: Vec<u8>,
        ciphertext: &mut Vec<u8>,
    ) -> Result<usize, RvError> {
        let cipher;

        match self.mode {
            CipherMode::CBC => {
                cipher = Cipher::sm4_cbc();
            }
            CipherMode::GCM => {
                cipher = Cipher::sm4_gcm();
            }
            _ => {
                return Err(RvError::ErrCryptoCipherOPNotSupported);
            }
        }

        if let None = self.ctx {
            // init adaptor ctx if it's not inited.
            let encrypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;
            let adaptor_ctx = AdaptorCTX {
                ctx: encrypter,
                tag_set: false,
                aad_set: false,
            };

            self.ctx = Some(adaptor_ctx);
        }

        if self.mode == CipherMode::GCM || self.mode == CipherMode::CCM {
            // set additional authenticated data before doing real jobs.
            if self.ctx.as_mut().unwrap().aad_set == false {
                if let Some(aad) = &self.aad {
                    self.ctx.as_mut().unwrap().ctx.aad_update(aad)?;
                    self.ctx.as_mut().unwrap().aad_set = true;
                }
            }
        }

        // do real jobs.
        // this Crypter::update returns a Result<usize, ErrorStack>, we simply ignore the detailed
        // error information by unwrapping it.
        // we also can't use the question mark operatior since the error codes are differently
        // defined in RustyVault and underlying adaptor, such as rust-openssl.
        let count = self
            .ctx
            .as_mut()
            .unwrap()
            .ctx
            .update(&plaintext, &mut ciphertext[..])?;
        Ok(count)
    }

    fn encrypt_final(&mut self, ciphertext: &mut Vec<u8>) -> Result<usize, RvError> {
        // Unlike encrypt_update() function, we don't do auto-initialization here.
        if self.ctx.is_none() {
            return Err(RvError::ErrCryptoCipherNotInited);
        }

        let count = self.ctx.as_mut().unwrap().ctx.finalize(ciphertext)?;

        if self.mode == CipherMode::GCM {
            // set tag for caller to obtain.
            if self.tag.is_some() {
                // tag should not be set before encrypt_final() is called.
                return Err(RvError::ErrCryptoCipherAEADTagPresent);
            }

            // 16-byte long is enough for all types of AEAD cipher tag.
            let mut tag: Vec<u8> = vec![0; 16];
            self.ctx.as_mut().unwrap().ctx.get_tag(&mut tag)?;
            self.tag = Some(tag);
        }

        Ok(count)
    }

    fn decrypt(&mut self, ciphertext: &Vec<u8>) -> Result<Vec<u8>, RvError> {
        match self.mode {
            CipherMode::CBC => {
                let plaintext = decrypt(Cipher::sm4_cbc(), &self.key, Some(&self.iv), ciphertext)?;
                return Ok(plaintext.to_vec());
            }
            CipherMode::GCM => {
                // SM4 is a fixed 128-bit cipher, the tag is 16-bytes long.
                let plaintext = decrypt_aead(
                    Cipher::sm4_gcm(),
                    &self.key,
                    Some(&self.iv),
                    &self.aad.clone().unwrap(),
                    ciphertext,
                    &self.tag.clone().unwrap(),
                )?;
                return Ok(plaintext.to_vec());
            }
            _ => Err(RvError::ErrCryptoCipherOPNotSupported),
        }
    }
    fn decrypt_update(
        &mut self,
        ciphertext: Vec<u8>,
        plaintext: &mut Vec<u8>,
    ) -> Result<usize, RvError> {
        let cipher;

        match self.mode {
            CipherMode::CBC => {
                cipher = Cipher::sm4_cbc();
            }
            CipherMode::GCM => {
                cipher = Cipher::sm4_gcm();
            }
            _ => {
                return Err(RvError::ErrCryptoCipherOPNotSupported);
            }
        }

        if self.ctx.is_none() {
            // init adaptor ctx if it's not inited.
            let encrypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;
            let adaptor_ctx = AdaptorCTX {
                ctx: encrypter,
                tag_set: false,
                aad_set: false,
            };

            self.ctx = Some(adaptor_ctx);
        }

        // set additional authenticated data before doing real jobs.
        if self.mode == CipherMode::GCM {
            if self.ctx.as_mut().unwrap().aad_set == false {
                if let Some(aad) = &self.aad {
                    self.ctx.as_mut().unwrap().ctx.aad_update(aad)?;
                    self.ctx.as_mut().unwrap().aad_set = true;
                }
            }
        }

        // do real jobs.
        // this Crypter::update returns a Result<usize, ErrorStack>, print detailed error if any.
        match self
            .ctx
            .as_mut()
            .unwrap()
            .ctx
            .update(&ciphertext, plaintext)
        {
            Ok(count) => {
                return Ok(count);
            }
            Err(err_stack) => {
                let errs = err_stack.errors();
                log::error!("{}", errs.len());
                for err in errs.iter() {
                    log::error!("{:?}", err.reason());
                }
                Err(RvError::ErrCryptoCipherUpdateFailed)
            }
        }
    }

    fn decrypt_final(&mut self, plaintext: &mut Vec<u8>) -> Result<usize, RvError> {
        // Unlike decrypt_update() function, we don't do auto-initialization here.
        if self.ctx.is_none() {
            return Err(RvError::ErrCryptoCipherNotInited);
        }

        // set tag before doing real jobs.
        if self.mode == CipherMode::GCM {
            if self.ctx.as_mut().unwrap().tag_set == false {
                if let Some(tag) = &self.tag {
                    self.ctx.as_mut().unwrap().ctx.set_tag(tag)?;
                    self.ctx.as_mut().unwrap().tag_set = true;
                } else {
                    // if tag is missing, then return an error.
                    return Err(RvError::ErrCryptoCipherNoTag);
                }
            }
        }

        match self.ctx.as_mut().unwrap().ctx.finalize(plaintext) {
            Ok(count) => {
                return Ok(count);
            }
            Err(err_stack) => {
                let errs = err_stack.errors();
                log::error!("{}", errs.len());
                for err in errs.iter() {
                    log::error!("{:?}", err.reason());
                }
                Err(RvError::ErrCryptoCipherFinalizeFailed)
            }
        }
    }
}

impl AEADCipher for SM4 {
    fn set_aad(&mut self, aad: Vec<u8>) -> Result<(), RvError> {
        self.aad = Some(aad.clone());
        Ok(())
    }
    fn get_tag(&mut self) -> Result<Vec<u8>, RvError> {
        if self.tag.is_none() {
            return Err(RvError::ErrCryptoCipherNoTag);
        }
        Ok(self.tag.clone().unwrap())
    }
    fn set_tag(&mut self, tag: Vec<u8>) -> Result<(), RvError> {
        self.tag = Some(tag.clone());
        Ok(())
    }
}
