//! This is the implementation of aes-gcm barrier, which uses aes-gcm block cipher to encrypt or
//! decrypt data before writing or reading data to or from specific storage backend.

use std::{
    any::Any,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use arc_swap::ArcSwap;
use better_default::Default;
use openssl::{
    hash::{MessageDigest, hash},
    symm::{Cipher, Crypter, Mode},
};
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::{
    Backend, BackendEntry, Storage, StorageEntry,
    barrier::{BARRIER_INIT_PATH, SecurityBarrier},
};
use crate::errors::RvError;

const EPOCH_SIZE: usize = 4;
const KEY_EPOCH: u8 = 1;
const AES_GCM_VERSION1: u8 = 0x1;
const AES_GCM_VERSION2: u8 = 0x2;
const AES_BLOCK_SIZE: usize = 16;

// the BarrierInit structure contains the encryption key, so it's zeroized anyway
// when it's dropped
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
#[serde(deny_unknown_fields)]
#[zeroize(drop)]
struct BarrierInit {
    version: u32,
    key: Vec<u8>,
}

#[derive(Debug, Clone, Default, Zeroize)]
#[zeroize(drop)]
struct BarrierInfo {
    #[default(true)]
    sealed: bool,
    key: Option<Vec<u8>>,
    #[default(AES_GCM_VERSION2)]
    aes_gcm_version_byte: u8,
}

pub struct AESGCMBarrier {
    barrier_info: ArcSwap<BarrierInfo>,
    backend: Arc<dyn Backend>,
}

#[async_trait::async_trait]
impl Storage for AESGCMBarrier {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        if self.barrier_info.load().sealed {
            return Err(RvError::ErrBarrierSealed);
        }

        let mut ret = self.backend.list(prefix).await?;
        ret.sort();

        Ok(ret)
    }

    async fn get(&self, key: &str) -> Result<Option<StorageEntry>, RvError> {
        if self.barrier_info.load().sealed {
            return Err(RvError::ErrBarrierSealed);
        }

        // Read the key from the backend
        let pe = self.backend.get(key).await?;
        if pe.is_none() {
            return Ok(None);
        }

        // Decrypt the ciphertext
        let plain = self.decrypt(key, pe.as_ref().unwrap().value.as_slice())?;
        let entry = StorageEntry {
            key: key.to_string(),
            value: plain,
        };

        Ok(Some(entry))
    }

    async fn put(&self, entry: &StorageEntry) -> Result<(), RvError> {
        if self.barrier_info.load().sealed {
            return Err(RvError::ErrBarrierSealed);
        }

        let ciphertext = self.encrypt(&entry.key, entry.value.as_slice())?;

        let be = BackendEntry {
            key: entry.key.clone(),
            value: ciphertext,
        };

        self.backend.put(&be).await?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        if self.barrier_info.load().sealed {
            return Err(RvError::ErrBarrierSealed);
        }
        self.backend.delete(key).await
    }

    async fn lock(&self, lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        self.backend.lock(lock_name).await
    }
}

#[async_trait::async_trait]
impl SecurityBarrier for AESGCMBarrier {
    async fn inited(&self) -> Result<bool, RvError> {
        let res = self.backend.get(BARRIER_INIT_PATH).await?;
        Ok(res.is_some())
    }

    // kek stands for key encryption key, it's used to encrypt the actual
    // encryption key, which is generated during the init() process.
    // The kek's zerization is handled in the caller.
    async fn init(&self, kek: &[u8]) -> Result<(), RvError> {
        let (min, max) = self.key_length_range();
        if kek.len() < min || kek.len() > max {
            return Err(RvError::ErrBarrierKeyInvalid);
        }

        // Check if already initialized
        let inited = self.inited().await?;
        if inited {
            return Err(RvError::ErrBarrierAlreadyInit);
        }

        // the encrypt_key variable will be zeroized automatically on drop
        let encrypt_key = self.generate_key()?;

        let barrier_init = BarrierInit {
            version: 1,
            key: encrypt_key.to_vec(),
        };

        let serialized_barrier_init = serde_json::to_string(&barrier_init)?;

        self.init_cipher(kek)?;

        let value = self.encrypt(BARRIER_INIT_PATH, serialized_barrier_init.as_bytes())?;

        let be = BackendEntry {
            key: BARRIER_INIT_PATH.to_string(),
            value,
        };

        self.backend.put(&be).await?;

        self.reset_cipher()?;

        Ok(())
    }

    fn generate_key(&self) -> Result<Zeroizing<Vec<u8>>, RvError> {
        let key_size = 2 * AES_BLOCK_SIZE;
        // will be zeroized on drop
        let mut buf = Zeroizing::new(vec![0u8; key_size]);

        thread_rng().fill(buf.deref_mut().as_mut_slice());
        Ok(buf)
    }

    fn key_length_range(&self) -> (usize, usize) {
        (AES_BLOCK_SIZE, 2 * AES_BLOCK_SIZE)
    }

    fn sealed(&self) -> Result<bool, RvError> {
        Ok(self.barrier_info.load().sealed)
    }

    async fn unseal(&self, kek: &[u8]) -> Result<(), RvError> {
        let sealed = self.sealed()?;
        if !sealed {
            return Ok(());
        }

        let entry = self.backend.get(BARRIER_INIT_PATH).await?;
        if entry.is_none() {
            return Err(RvError::ErrBarrierNotInit);
        }

        self.init_cipher(kek)?;

        let value = self.decrypt(BARRIER_INIT_PATH, entry.unwrap().value.as_slice());
        if value.is_err() {
            return Err(RvError::ErrBarrierUnsealFailed);
        }
        let barrier_init: BarrierInit = serde_json::from_slice(value.unwrap().as_slice())?;

        // the barrier_init.key is the real encryption key generated in init().
        // the whole barrier_init will be zeroized on drop, so there is no special
        // zeroizing logic on barrier_init.key.
        self.init_cipher(barrier_init.key.as_slice())?;

        let mut barrier_info = (*self.barrier_info.load_full()).clone();
        barrier_info.sealed = false;
        self.barrier_info.store(Arc::new(barrier_info));

        Ok(())
    }

    fn seal(&self) -> Result<(), RvError> {
        self.reset_cipher()?;
        let mut barrier_info = (*self.barrier_info.load_full()).clone();
        barrier_info.sealed = true;
        self.barrier_info.store(Arc::new(barrier_info));
        Ok(())
    }

    fn derive_hmac_key(&self) -> Result<Vec<u8>, RvError> {
        let barrier_info = self.barrier_info.load();
        if barrier_info.key.is_none() {
            return Err(RvError::ErrBarrierNotInit);
        }

        if self.sealed()? {
            return Err(RvError::ErrBarrierSealed);
        }

        let key = Zeroizing::new(barrier_info.key.clone().unwrap());

        let ret = hash(MessageDigest::sha256(), key.deref().as_slice())?;
        Ok(ret.to_vec())
    }

    fn as_storage(&self) -> &dyn Storage {
        self
    }
}

impl AESGCMBarrier {
    pub fn new(physical: Arc<dyn Backend>) -> Self {
        Self {
            backend: physical,
            barrier_info: ArcSwap::from_pointee(BarrierInfo::default()),
        }
    }

    fn init_cipher(&self, key: &[u8]) -> Result<(), RvError> {
        let mut barrier_info = (*self.barrier_info.load_full()).clone();
        barrier_info.key = Some(key.to_vec());
        self.barrier_info.store(Arc::new(barrier_info));
        Ok(())
    }

    fn reset_cipher(&self) -> Result<(), RvError> {
        let mut barrier_info = (*self.barrier_info.load_full()).clone();
        // Zeroize it explicitly
        barrier_info.key.zeroize();
        barrier_info.key = None;
        self.barrier_info.store(Arc::new(barrier_info));
        Ok(())
    }

    fn encrypt(&self, path: &str, plaintext: &[u8]) -> Result<Vec<u8>, RvError> {
        let barrier_info = self.barrier_info.load();
        if barrier_info.key.is_none() {
            return Err(RvError::ErrBarrierNotInit);
        }

        let cipher = Cipher::aes_256_gcm();
        let iv_len = cipher.iv_len().unwrap_or(0);
        let tag_len = 16;
        let block_size = cipher.block_size();

        // XXX: the cloned variable 'key' will be zeroized automatically on drop
        let key = Zeroizing::new(barrier_info.key.clone().unwrap());

        let size: usize = EPOCH_SIZE + 1 + iv_len + plaintext.len() + tag_len;
        let mut out = vec![0u8; size + block_size];
        out[3] = KEY_EPOCH;
        out[4] = barrier_info.aes_gcm_version_byte;

        // Generate a random nonce
        let mut nonce = Zeroizing::new(vec![0u8; iv_len]);
        let iv = match iv_len {
            0 => None,
            _ => {
                thread_rng().fill(nonce.deref_mut().as_mut_slice());
                out[5..5 + iv_len].copy_from_slice(nonce.deref().as_slice());
                Some(nonce.deref().as_slice())
            }
        };

        let mut encrypter = Crypter::new(cipher, Mode::Encrypt, key.deref().as_slice(), iv)?;

        encrypter.pad(false);

        if barrier_info.aes_gcm_version_byte == AES_GCM_VERSION2 {
            encrypter.aad_update(path.as_bytes())?;
        }

        let mut count = encrypter.update(plaintext, &mut out[EPOCH_SIZE + 1 + iv_len..])?;
        count += encrypter.finalize(&mut out[EPOCH_SIZE + 1 + iv_len + count..])?;
        out.truncate(EPOCH_SIZE + 1 + iv_len + count + tag_len);

        encrypter.get_tag(&mut out[EPOCH_SIZE + 1 + iv_len + count..])?;

        Ok(out)
    }

    fn decrypt(&self, path: &str, ciphertext: &[u8]) -> Result<Vec<u8>, RvError> {
        let barrier_info = self.barrier_info.load();
        if barrier_info.key.is_none() {
            return Err(RvError::ErrBarrierNotInit);
        }

        if ciphertext[0] != 0
            || ciphertext[1] != 0
            || ciphertext[2] != 0
            || ciphertext[3] != KEY_EPOCH
        {
            return Err(RvError::ErrBarrierEpochMismatch);
        }

        let cipher = Cipher::aes_256_gcm();
        let block_size = cipher.block_size();
        let iv_len = cipher.iv_len().unwrap_or(0);
        let tag_len = 16;

        let key = Zeroizing::new(barrier_info.key.clone().unwrap());

        let iv = match iv_len {
            0 => None,
            _ => Some(&ciphertext[5..5 + iv_len]),
        };

        let mut decrypter = Crypter::new(cipher, Mode::Decrypt, key.deref().as_slice(), iv)?;

        decrypter.pad(false);

        match ciphertext[4] {
            AES_GCM_VERSION1 => {}
            AES_GCM_VERSION2 => {
                decrypter.aad_update(path.as_bytes())?;
            }
            _ => {
                return Err(RvError::ErrBarrierVersionMismatch);
            }
        };

        let raw = &ciphertext[5 + iv_len..ciphertext.len() - tag_len];
        let tag = &ciphertext[ciphertext.len() - tag_len..ciphertext.len()];
        let size = ciphertext.len() - 5 - iv_len - tag_len;
        let mut out = vec![0u8; size + block_size];

        let mut count = decrypter.update(raw, &mut out)?;

        decrypter.set_tag(tag)?;

        count += decrypter.finalize(&mut out[count..])?;
        out.truncate(count);

        Ok(out)
    }
}
