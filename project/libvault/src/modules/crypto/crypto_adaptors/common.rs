//! This module contains some common functions used by openssl and tongsuo adaptors.
//! Functions in this module SHOULD NOT be used directly by applications.

use openssl::symm::Cipher;

use crate::modules::{
    RvError,
    crypto::{AESKeySize, CipherMode},
};

macro_rules! common_aes_set_aad {
    ($aes: expr, $aad: expr) => {
        $aes.aad = Some($aad.clone());
        return Ok(());
    };
}

macro_rules! common_aes_get_tag {
    ($aes: expr) => {
        if $aes.tag.is_none() {
            return Err(RvError::ErrCryptoCipherNoTag);
        }
        return Ok($aes.tag.clone().unwrap());
    };
}

macro_rules! common_aes_set_tag {
    ($aes: expr, $tag: expr) => {
        $aes.tag = Some($tag.clone());
        return Ok(());
    };
}

macro_rules! common_aes_new {
    ($keygen: expr, $size: expr, $mode: expr, $key: expr, $iv: expr) => {
        // default algorithm: AES128 + CBC.
        let mut k_size = AESKeySize::AES128;
        let mut c_mode = CipherMode::CBC;
        let aes_key: Vec<u8>;
        let aes_iv: Vec<u8>;

        if let Some(x) = $size {
            k_size = x;
        }

        if let Some(y) = $mode {
            c_mode = y;
        }

        if $keygen == false {
            match ($key, $iv) {
                (Some(x), Some(y)) => {
                    aes_key = x.clone();
                    aes_iv = y.clone();
                }
                _ => return Err(RvError::ErrCryptoCipherInitFailed),
            }
        } else {
            // generate new key and iv based on k_size.
            match k_size {
                AESKeySize::AES128 => {
                    // for aes-128, key is 16 bytes and iv is 16 bytes
                    let mut buf = [0; 16];
                    let mut buf2 = [0; 16];
                    rand_priv_bytes(&mut buf)?;
                    aes_key = buf.to_vec();
                    rand_priv_bytes(&mut buf2)?;
                    aes_iv = buf2.to_vec();
                }
                AESKeySize::AES192 => {
                    // for aes-192, key is 24 bytes and iv is 16 bytes
                    let mut buf = [0; 24];
                    let mut buf2 = [0; 16];
                    rand_priv_bytes(&mut buf)?;
                    aes_key = buf.to_vec();
                    rand_priv_bytes(&mut buf2)?;
                    aes_iv = buf2.to_vec();
                }
                AESKeySize::AES256 => {
                    // for aes-256, key is 32 bytes and iv is 16 bytes
                    let mut buf = [0; 32];
                    let mut buf2 = [0; 16];
                    rand_priv_bytes(&mut buf)?;
                    aes_key = buf.to_vec();
                    rand_priv_bytes(&mut buf2)?;
                    aes_iv = buf2.to_vec();
                }
            }
        }

        return Ok(AES {
            alg: (k_size, c_mode),
            key: aes_key,
            iv: aes_iv,
            aad: None,
            ctx: None,
            tag: None,
        });
    };
}

macro_rules! common_get_key_iv {
    ($aes: expr) => {
        return ($aes.key.clone(), $aes.iv.clone());
    };
}

pub fn common_internal_get_cipher_alg(
    alg: &(AESKeySize, CipherMode),
) -> Result<(Cipher, bool), RvError> {
    let cipher;
    let mut aead = false;

    match alg {
        (AESKeySize::AES128, CipherMode::CBC) => cipher = Cipher::aes_128_cbc(),
        (AESKeySize::AES192, CipherMode::CBC) => cipher = Cipher::aes_192_cbc(),
        (AESKeySize::AES256, CipherMode::CBC) => cipher = Cipher::aes_256_cbc(),
        (AESKeySize::AES128, CipherMode::GCM) => {
            cipher = Cipher::aes_128_gcm();
            aead = true;
        }
        (AESKeySize::AES192, CipherMode::GCM) => {
            cipher = Cipher::aes_192_gcm();
            aead = true;
        }
        (AESKeySize::AES256, CipherMode::GCM) => {
            cipher = Cipher::aes_256_gcm();
            aead = true;
        }
        _ => return Err(RvError::ErrCryptoCipherOPNotSupported),
    }

    Ok((cipher, aead))
}

macro_rules! common_aes_encrypt {
    ($aes: expr, $plaintext: expr) => {
        let cipher;
        let aead: bool;

        (cipher, aead) = common::common_internal_get_cipher_alg(&$aes.alg)?;

        if aead == false {
            let ciphertext = encrypt(cipher, &$aes.key, Some(&$aes.iv), $plaintext)?;
            return Ok(ciphertext.to_vec());
        } else {
            // aes_xxx_gcm's tag is at most 16-bytes long.
            let tag: &mut [u8] = &mut [0; 16];
            let ciphertext = encrypt_aead(
                cipher,
                &$aes.key,
                Some(&$aes.iv),
                &$aes.aad.clone().unwrap(),
                $plaintext,
                tag,
            )?;
            $aes.tag = Some(tag.to_vec());
            return Ok(ciphertext.to_vec());
        }
    };
}

macro_rules! common_aes_decrypt {
    ($aes: expr, $ciphertext: expr) => {
        let cipher;
        let aead: bool;

        (cipher, aead) = common::common_internal_get_cipher_alg(&$aes.alg)?;

        if aead == false {
            let plaintext = decrypt(cipher, &$aes.key, Some(&$aes.iv), $ciphertext)?;
            return Ok(plaintext.to_vec());
        } else {
            let plaintext = decrypt_aead(
                cipher,
                &$aes.key,
                Some(&$aes.iv),
                &$aes.aad.clone().unwrap(),
                $ciphertext,
                &$aes.tag.clone().unwrap(),
            )?;
            return Ok(plaintext.to_vec());
        }
    };
}

macro_rules! common_aes_encrypt_update {
    ($aes: expr, $plaintext: expr, $ciphertext: expr) => {
        let cipher;

        match $aes.alg {
            (AESKeySize::AES128, CipherMode::CBC) => {
                cipher = Cipher::aes_128_cbc();
            }
            (AESKeySize::AES128, CipherMode::GCM) => {
                cipher = Cipher::aes_128_gcm();
            }
            _ => {
                return Err(RvError::ErrCryptoCipherOPNotSupported);
            }
        }

        if $aes.ctx.is_none() {
            // init adaptor ctx if it's not inited.
            let encrypter = Crypter::new(cipher, Mode::Encrypt, &$aes.key, Some(&$aes.iv))?;
            let adaptor_ctx = AdaptorCTX {
                ctx: encrypter,
                tag_set: false,
                aad_set: false,
            };

            $aes.ctx = Some(adaptor_ctx);
        }

        if $aes.alg.1 == CipherMode::GCM || $aes.alg.1 == CipherMode::CCM {
            // set additional authenticated data before doing real jobs.
            if $aes.ctx.as_mut().unwrap().aad_set == false {
                if let Some(aad) = &$aes.aad {
                    $aes.ctx.as_mut().unwrap().ctx.aad_update(aad)?;
                    $aes.ctx.as_mut().unwrap().aad_set = true;
                }
            }
        }

        // do real jobs.
        // this Crypter::update returns a Result<usize, ErrorStack>, we simply ignore the detailed
        // error information by unwrapping it.
        // we also can't use the question mark operatior since the error codes are differently
        // defined in RustyVault and underlying adaptor, such as rust-openssl.
        let count = $aes
            .ctx
            .as_mut()
            .unwrap()
            .ctx
            .update(&$plaintext, &mut $ciphertext[..])?;

        return Ok(count);
    };
}

macro_rules! common_aes_encrypt_final {
    ($aes: expr, $ciphertext: expr) => {
        // Unlike encrypt_update() function, we don't do auto-initialization here.
        if $aes.ctx.is_none() {
            return Err(RvError::ErrCryptoCipherNotInited);
        }

        let count = $aes.ctx.as_mut().unwrap().ctx.finalize($ciphertext)?;

        if $aes.alg.1 == CipherMode::GCM {
            // set tag for caller to obtain.
            if $aes.tag.is_some() {
                // tag should not be set before encrypt_final() is called.
                return Err(RvError::ErrCryptoCipherAEADTagPresent);
            }

            // 16-byte long is enough for all types of AEAD cipher tag.
            // TODO: this is for AES-128-GCM only.
            let mut tag: Vec<u8> = vec![0; 16];
            $aes.ctx.as_mut().unwrap().ctx.get_tag(&mut tag)?;
            $aes.tag = Some(tag);
        }

        return Ok(count);
    };
}

macro_rules! common_aes_decrypt_update {
    ($aes: expr, $ciphertext: expr, $plaintext: expr) => {
        let cipher;

        match $aes.alg {
            (AESKeySize::AES128, CipherMode::CBC) => {
                cipher = Cipher::aes_128_cbc();
            }
            (AESKeySize::AES128, CipherMode::GCM) => {
                cipher = Cipher::aes_128_gcm();
            }
            _ => {
                return Err(RvError::ErrCryptoCipherOPNotSupported);
            }
        }

        if $aes.ctx.is_none() {
            // init adaptor ctx if it's not inited.
            let encrypter = Crypter::new(cipher, Mode::Decrypt, &$aes.key, Some(&$aes.iv))?;
            let adaptor_ctx = AdaptorCTX {
                ctx: encrypter,
                tag_set: false,
                aad_set: false,
            };

            $aes.ctx = Some(adaptor_ctx);
        }

        // set additional authenticated data before doing real jobs.
        if $aes.alg.1 == CipherMode::GCM {
            if $aes.ctx.as_mut().unwrap().aad_set == false {
                if let Some(aad) = &$aes.aad {
                    $aes.ctx.as_mut().unwrap().ctx.aad_update(aad)?;
                    $aes.ctx.as_mut().unwrap().aad_set = true;
                }
            }
        }

        // do real jobs.
        // this Crypter::update returns a Result<usize, ErrorStack>, print detailed error if any.
        match $aes
            .ctx
            .as_mut()
            .unwrap()
            .ctx
            .update(&$ciphertext, $plaintext)
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
                return Err(RvError::ErrCryptoCipherUpdateFailed);
            }
        }
    };
}

macro_rules! common_aes_decrypt_final {
    ($aes: expr, $plaintext: expr) => {
        // Unlike decrypt_update() function, we don't do auto-initialization here.
        if $aes.ctx.is_none() {
            return Err(RvError::ErrCryptoCipherNotInited);
        }

        // set tag before doing real jobs.
        if $aes.alg.1 == CipherMode::GCM {
            if $aes.ctx.as_mut().unwrap().tag_set == false {
                if let Some(tag) = &$aes.tag {
                    $aes.ctx.as_mut().unwrap().ctx.set_tag(tag)?;
                    $aes.ctx.as_mut().unwrap().tag_set = true;
                } else {
                    // if tag is missing, then return an error.
                    return Err(RvError::ErrCryptoCipherNoTag);
                }
            }
        }

        match $aes.ctx.as_mut().unwrap().ctx.finalize($plaintext) {
            Ok(count) => {
                return Ok(count);
            }
            Err(err_stack) => {
                let errs = err_stack.errors();
                log::error!("{}", errs.len());
                for err in errs.iter() {
                    log::error!("{:?}", err.reason());
                }
                return Err(RvError::ErrCryptoCipherFinalizeFailed);
            }
        }
    };
}
