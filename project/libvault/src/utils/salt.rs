//! This module is a Rust replica of
//! <https://github.com/hashicorp/vault/blob/main/sdk/helper/salt/salt.go>

use better_default::Default;
use derivative::Derivative;
use openssl::{
    hash::{hash, MessageDigest},
    nid::Nid,
    pkey::PKey,
    sign::Signer,
};

use super::generate_uuid;
use crate::{
    errors::RvError,
    storage::{Storage, StorageEntry},
};

static DEFAULT_LOCATION: &str = "salt";

#[derive(Debug, Clone, Default)]
pub struct Salt {
    pub config: Config,
    #[default(generate_uuid())]
    pub salt: String,
    #[default(true)]
    pub generated: bool,
}

#[derive(Derivative, Default)]
#[derivative(Debug, Clone)]
pub struct Config {
    #[default(DEFAULT_LOCATION.to_string())]
    pub location: String,
    #[derivative(Debug = "ignore")]
    #[default(MessageDigest::sha256())]
    pub hash_type: MessageDigest,
    #[derivative(Debug = "ignore")]
    #[default(MessageDigest::sha256())]
    pub hmac_type: MessageDigest,
}

#[maybe_async::maybe_async]
impl Salt {
    pub async fn new(storage: Option<&dyn Storage>, config: Option<&Config>) -> Result<Self, RvError> {
        let mut salt = Salt::default();
        if let Some(c) = config {
            if salt.config.location != c.location && !c.location.is_empty() {
                salt.config.location.clone_from(&c.location);
            }

            if salt.config.hash_type != c.hash_type {
                salt.config.hash_type = c.hash_type;
            }

            if salt.config.hmac_type != c.hmac_type {
                salt.config.hmac_type = c.hmac_type;
            }
        }

        if let Some(s) = storage {
            if let Some(raw) = s.get(&salt.config.location).await? {
                salt.salt = String::from_utf8_lossy(&raw.value).to_string();
                salt.generated = false;
            } else {
                let entry = StorageEntry { key: salt.config.location.clone(), value: salt.salt.as_bytes().to_vec() };

                s.put(&entry).await?;
            }
        }

        Ok(salt)
    }

    pub fn new_nonpersistent() -> Self {
        let mut salt = Salt::default();
        salt.config.location = "".to_string();
        salt
    }

    pub fn get_hmac(&self, data: &str) -> Result<String, RvError> {
        let pkey = PKey::hmac(self.salt.as_bytes())?;
        let mut signer = Signer::new(self.config.hmac_type, &pkey)?;
        signer.update(data.as_bytes())?;
        let hmac = signer.sign_to_vec()?;
        Ok(hex::encode(hmac.as_slice()))
    }

    pub fn get_identified_hamc(&self, data: &str) -> Result<String, RvError> {
        let hmac_type = match self.config.hmac_type.type_() {
            Nid::SHA256 => "hmac-sha256",
            Nid::SM3 => "hmac-sm3",
            Nid::MD5 => "hmac-md5",
            _ => "hmac-unknown",
        };

        let hmac = self.get_hmac(data)?;

        Ok(format!("{hmac_type}:{hmac}"))
    }

    pub fn get_hash(&self, data: &str) -> Result<String, RvError> {
        let ret = hash(self.config.hash_type, data.as_bytes())?;
        let bytes = ret.to_vec();
        Ok(hex::encode(bytes.as_slice()))
    }

    pub fn salt_id(&self, id: &str) -> Result<String, RvError> {
        let comb = format!("{}{}", self.salt, id);
        self.get_hash(&comb)
    }

    pub fn did_generate(&self) -> bool {
        self.generated
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use rand::{thread_rng, Rng};

    use super::*;
    use crate::{
        storage::{barrier::SecurityBarrier, barrier_aes_gcm, barrier_view},
        test_utils::new_test_backend,
    };

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_salt() {
        // init the storage backend
        let backend = new_test_backend("test_salt");

        let mut key = vec![0u8; 32];
        thread_rng().fill(key.as_mut_slice());
        let aes_gcm_view = barrier_aes_gcm::AESGCMBarrier::new(backend);

        let init = aes_gcm_view.init(key.as_slice()).await;
        assert!(init.is_ok());

        let result = aes_gcm_view.unseal(key.as_slice()).await;
        assert!(result.is_ok());

        let view = barrier_view::BarrierView::new(Arc::new(aes_gcm_view), "test");

        //test salt
        let salt = Salt::new(Some(&view), None).await;
        assert!(salt.is_ok());

        let salt = salt.unwrap();
        assert!(salt.did_generate());

        let ss = view.get(DEFAULT_LOCATION).await;
        assert!(ss.is_ok());
        assert!(ss.unwrap().is_some());

        let salt2 = Salt::new(Some(&view), Some(&salt.config)).await;
        assert!(salt2.is_ok());

        let salt2 = salt2.unwrap();
        assert!(!salt2.did_generate());

        assert_eq!(salt.salt, salt2.salt);

        let id = "foobarbaz";
        let sid1 = salt.salt_id(id);
        let sid2 = salt2.salt_id(id);
        assert!(sid1.is_ok());
        assert!(sid2.is_ok());

        let sid1 = sid1.unwrap();
        let sid2 = sid2.unwrap();
        assert_eq!(sid1, sid2);
        assert_eq!(sid1.len(), salt.config.hash_type.size() * 2);
    }
}
