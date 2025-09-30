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
    hash::{hash, MessageDigest},
    symm::{Cipher, Crypter, Mode},
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::{
    barrier::{SecurityBarrier, BARRIER_INIT_PATH},
    Backend, BackendEntry, Storage, StorageEntry,
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

#[maybe_async::maybe_async]
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
        let entry = StorageEntry { key: key.to_string(), value: plain };

        Ok(Some(entry))
    }

    async fn put(&self, entry: &StorageEntry) -> Result<(), RvError> {
        if self.barrier_info.load().sealed {
            return Err(RvError::ErrBarrierSealed);
        }

        let ciphertext = self.encrypt(&entry.key, entry.value.as_slice())?;

        let be = BackendEntry { key: entry.key.clone(), value: ciphertext };

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

#[maybe_async::maybe_async]
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

        let barrier_init = BarrierInit { version: 1, key: encrypt_key.to_vec() };

        let serialized_barrier_init = serde_json::to_string(&barrier_init)?;

        self.init_cipher(kek)?;

        let value = self.encrypt(BARRIER_INIT_PATH, serialized_barrier_init.as_bytes())?;

        let be = BackendEntry { key: BARRIER_INIT_PATH.to_string(), value };

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
        Self { backend: physical, barrier_info: ArcSwap::from_pointee(BarrierInfo::default()) }
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

        if ciphertext[0] != 0 || ciphertext[1] != 0 || ciphertext[2] != 0 || ciphertext[3] != KEY_EPOCH {
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

#[cfg(test)]
mod test {
    use super::{super::*, *};
    use crate::test_utils::new_test_backend;

    #[test]
    fn test_barrier_encrypt_decrypt() {
        let backend = new_test_backend("test_encrypt_decrypt");

        let mut key = vec![0u8; 32];
        thread_rng().fill(key.as_mut_slice());

        let barrier = AESGCMBarrier {
            backend,
            barrier_info: ArcSwap::from_pointee(BarrierInfo { sealed: true, key: Some(key), ..Default::default() }),
        };

        let path = "test/";
        let plaintext = "rusty vault test";
        let encrypt_data = barrier.encrypt(path, plaintext.as_bytes());
        assert!(encrypt_data.is_ok());
        let ciphertext = encrypt_data.unwrap();
        let decrypt_data = barrier.decrypt(path, ciphertext.as_slice());
        assert!(decrypt_data.is_ok());
        assert_eq!(plaintext.as_bytes(), decrypt_data.unwrap());

        let decrypt_data = barrier.decrypt("test2/", ciphertext.as_slice());
        assert!(decrypt_data.is_err());
    }

    #[test]
    fn test_barrier_decrypt() {
        let backend = new_test_backend("test_decrypt");

        let key = vec![
            121, 133, 170, 204, 71, 77, 160, 134, 22, 37, 254, 206, 120, 206, 143, 197, 150, 83, 5, 45, 121, 51, 124,
            110, 162, 1, 9, 51, 16, 75, 157, 129,
        ];

        let barrier = AESGCMBarrier {
            backend,
            barrier_info: ArcSwap::from_pointee(BarrierInfo { sealed: true, key: Some(key), ..Default::default() }),
        };

        // AES_GCM_VERSION1
        let ciphertext = &[
            0, 0, 0, 1, 1, 99, 115, 28, 164, 208, 39, 20, 70, 150, 217, 80, 159, 80, 251, 42, 49, 32, 136, 109, 90,
            160, 217, 227, 252, 159, 54, 194, 68, 146, 37, 88, 57, 225, 144, 96, 105, 160, 187, 112, 145, 175, 24, 89,
            33,
        ];
        let res = barrier.decrypt("test/", ciphertext);
        assert!(res.is_ok());

        // AES_GCM_VERSION2
        let ciphertext2 = &[
            0, 0, 0, 1, 2, 146, 4, 80, 230, 214, 110, 208, 132, 3, 230, 0, 186, 251, 246, 9, 166, 168, 126, 134, 95,
            20, 28, 253, 33, 169, 84, 146, 234, 7, 140, 98, 119, 42, 14, 35, 26, 213, 131, 32, 139, 216, 68, 148, 136,
        ];
        let plaintext = "rusty vault test";

        let res = barrier.decrypt("test2/", ciphertext2);
        assert!(res.is_ok());
        assert_eq!(plaintext.as_bytes(), res.unwrap());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_barrier_aes256_gcm() {
        let backend = new_test_backend("test_barriew_aes256_gcm");

        let barrier = AESGCMBarrier::new(backend.clone());

        let inited = barrier.inited().await;
        assert!(inited.is_ok());
        assert!(!inited.unwrap());

        let sealed = barrier.sealed();
        assert!(sealed.is_ok());
        assert!(sealed.unwrap());

        let mut key = vec![0u8; 32];
        thread_rng().fill(key.as_mut_slice());
        let init = barrier.init(key.as_slice()).await;
        assert!(init.is_ok());

        let sealed = barrier.sealed();
        assert!(sealed.is_ok());
        assert!(sealed.unwrap());

        let unseal = barrier.unseal(key.as_slice()).await;
        assert!(unseal.is_ok());

        let sealed = barrier.sealed();
        assert!(sealed.is_ok());
        assert!(!sealed.unwrap());

        let seal = barrier.seal();
        assert!(seal.is_ok());

        let sealed = barrier.sealed();
        assert!(sealed.is_ok());
        assert!(sealed.unwrap());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_barrier_storage_api() {
        let backend = new_test_backend("test_barriew_storage_api");

        let barrier = AESGCMBarrier::new(backend.clone());

        let mut key = vec![0u8; 32];
        thread_rng().fill(key.as_mut_slice());
        let init = barrier.init(key.as_slice()).await;
        assert!(init.is_ok());

        let unseal = barrier.unseal(key.as_slice()).await;
        assert!(unseal.is_ok());

        let list = barrier.list("/bin").await;
        assert!(list.is_err());

        let list = barrier.list("").await;
        assert!(list.is_ok());
        assert_eq!(list.unwrap().len(), 1);

        let list = barrier.list("xxx").await;
        assert!(list.is_ok());
        assert_eq!(list.unwrap().len(), 0);

        let get = barrier.get("").await;
        assert!(get.is_ok());
        assert!(get.unwrap().is_none());

        let get = barrier.get("bar").await;
        assert!(get.is_ok());
        assert!(get.unwrap().is_none());

        let get = barrier.get("/").await;
        assert!(get.is_err());

        let entry1 = StorageEntry { key: "bar".to_string(), value: "test1".as_bytes().to_vec() };
        let entry2 = StorageEntry { key: "bar/foo".to_string(), value: "test2".as_bytes().to_vec() };
        let entry3 = StorageEntry { key: "bar/foo/goo".to_string(), value: "test3".as_bytes().to_vec() };

        let put = barrier.put(&entry1).await;
        assert!(put.is_ok());

        let put = barrier.put(&entry2).await;
        assert!(put.is_ok());

        let put = barrier.put(&entry3).await;
        assert!(put.is_ok());

        // test the root
        let keys = barrier.list("").await;
        assert!(keys.is_ok());
        let keys = keys.unwrap();
        assert_eq!(keys.len(), 3);
        assert!(
            keys.join("") == "barbarrier/bar/"
                || keys.join("") == "barbar/barrier/"
                || keys.join("") == "bar/barbarrier/"
                || keys.join("") == "barrier/bar/bar"
                || keys.join("") == "barrier/barbar/"
                || keys.join("") == "bar/barrier/bar"
        );
        let get = barrier.get("bar").await;
        assert!(get.is_ok());
        assert_eq!(get.unwrap().unwrap().value, "test1".as_bytes());

        // test bar/
        let keys = barrier.list("bar/").await;
        assert!(keys.is_ok());
        let keys = keys.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.join("") == "foofoo/" || keys.join("") == "foo/foo");
        let get = barrier.get("bar/foo").await;
        assert!(get.is_ok());
        assert_eq!(get.unwrap().unwrap().value, "test2".as_bytes());

        // test bar/foo/
        let keys = barrier.list("bar/foo/").await;
        assert!(keys.is_ok());
        let keys = keys.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "goo".to_string());
        let get = barrier.get("bar/foo/goo").await;
        assert!(get.is_ok());
        assert_eq!(get.unwrap().unwrap().value, "test3".as_bytes());

        // backend entry value should be encrypted
        let get = barrier.backend.get("bar/foo/goo").await;
        assert!(get.is_ok());
        assert_ne!(get.unwrap().unwrap().value, "test3".as_bytes());

        // after deletion, should not be able to get the entry
        let delete = barrier.delete("bar").await;
        assert!(delete.is_ok());
        let get = barrier.get("bar").await;
        assert!(get.is_ok());
        assert!(get.unwrap().is_none());
        let keys = barrier.list("").await;
        assert!(keys.is_ok());
        let keys = keys.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.join("") == "barrier/bar/" || keys.join("") == "bar/barrier/");

        let seal = barrier.seal();
        assert!(seal.is_ok());

        // after sealing, all API operations should result in errors
        let keys = barrier.list("").await;
        assert!(keys.is_err());
        let put = barrier.put(&entry1).await;
        assert!(put.is_err());
        let get = barrier.get("bar/foo").await;
        assert!(get.is_err());
        let delete = barrier.delete("bar/foo").await;
        assert!(delete.is_err());
    }
}
