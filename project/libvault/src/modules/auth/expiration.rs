//! This file contains the implementation of the ExpirationManager, which is responsible
//! for managing lease entries and their expiration. It includes functionalities to register,
//! renew, and revoke leases, as well as to check for expired leases and handle them accordingly.

use std::{
    cmp::Reverse,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, RwLock, Weak},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use better_default::Default;
use crossbeam_channel::{select, tick};
use priority_queue::PriorityQueue;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{token_store::TokenEntry, TokenStore};
use crate::{
    core::Core,
    errors::RvError,
    logical::{lease::calculate_ttl, Auth, Request, Response, SecretData},
    router::Router,
    rv_error_string,
    storage::{barrier_view::BarrierView, Storage, StorageEntry},
    utils::{
        deserialize_system_time, generate_uuid, serialize_system_time,
        token_util::{DEFAULT_LEASE_TTL, MAX_LEASE_TTL},
    },
};

pub const EXPIRATION_SUB_PATH: &str = "expire/";
pub const LEASE_VIEW_PREFIX: &str = "id/";
pub const TOKEN_VIEW_PREFIX: &str = "token/";
pub const MAX_REVOKE_ATTEMPTS: u32 = 6;
pub const REVOKE_RETRY_SECS: Duration = Duration::from_secs(10);
pub const MIN_REVOKE_DELAY_SECS: Duration = Duration::from_secs(5);
pub const MAX_LEASE_DURATION_SECS: Duration = Duration::from_secs(30 * 24 * 60 * 60);
pub const DEFAULT_LEASE_DURATION_SECS: Duration = Duration::from_secs(24 * 60 * 60);

/// Represents an old lease entry that may need to be converted to the new format.
#[derive(Eq, Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
struct OldLeaseEntry {
    #[serde(default)]
    pub lease_id: String,
    pub client_token: String,
    pub path: String,
    pub data: Option<HashMap<String, Value>>,
    pub secret: Option<SecretData>,
    pub auth: Option<Auth>,
    #[default(SystemTime::now())]
    #[serde(serialize_with = "serialize_system_time", deserialize_with = "deserialize_system_time")]
    pub issue_time: SystemTime,
    #[default(SystemTime::now())]
    #[serde(serialize_with = "serialize_system_time", deserialize_with = "deserialize_system_time")]
    pub expire_time: SystemTime,
}

/// Represents a lease entry with all the necessary information.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LeaseEntry {
    #[serde(default)]
    pub lease_id: String,
    pub client_token: String,
    pub path: String,
    pub data: Map<String, Value>,
    pub secret: Option<SecretData>,
    pub auth: Option<Auth>,
    #[default(SystemTime::now())]
    #[serde(serialize_with = "serialize_system_time", deserialize_with = "deserialize_system_time")]
    pub issue_time: SystemTime,
    #[default(SystemTime::UNIX_EPOCH)]
    #[serde(serialize_with = "serialize_system_time", deserialize_with = "deserialize_system_time")]
    pub expire_time: SystemTime,
    #[serde(default)]
    pub revoke_err: String,
}

/// The ExpirationManager is responsible for managing lease entries and their expiration.
pub struct ExpirationManager {
    pub self_ptr: Weak<Self>,
    pub router: Arc<Router>,
    pub id_view: Arc<BarrierView>,
    pub token_view: Arc<BarrierView>,
    pub token_store: RwLock<Weak<TokenStore>>,
    queue: Arc<RwLock<PriorityQueue<Arc<LeaseEntry>, Reverse<u128>>>>,
}

impl Hash for LeaseEntry {
    /// Implements hash function for lease entries based on unique attributes.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lease_id.hash(state);
        self.client_token.hash(state);
        self.path.hash(state);
    }
}

impl PartialEq for LeaseEntry {
    /// Implements partial equality checking for lease entries.
    fn eq(&self, other: &Self) -> bool {
        self.lease_id == other.lease_id && self.client_token == other.client_token && self.path == other.path
    }
}

impl Eq for LeaseEntry {}

impl LeaseEntry {
    /// Checks if the lease entry is renewable.
    fn renewable(&self) -> bool {
        self.expire_time >= SystemTime::now()
            && self.secret.as_ref().map_or(true, |s| s.renewable())
            && self.auth.as_ref().map_or(true, |a| a.renewable())
    }

    #[allow(dead_code)]
    fn is_non_expiring(&self) -> bool {
        self.auth.as_ref().map_or(false, |a| a.enabled() && a.policies.len() == 1 && a.policies[0] == "root")
    }

    #[allow(dead_code)]
    fn is_irrevocable(&self) -> bool {
        !self.revoke_err.is_empty()
    }

    #[allow(dead_code)]
    fn is_incorrectly_non_expiring(&self) -> bool {
        self.expire_time == SystemTime::UNIX_EPOCH && !self.is_non_expiring()
    }
}

impl ExpirationManager {
    /// Creates a new ExpirationManager instance.
    pub fn new(core: &Core) -> Result<ExpirationManager, RvError> {
        let Some(system_view) = core.state.load().system_view.as_ref().cloned() else {
            return Err(RvError::ErrBarrierSealed);
        };

        let id_view = system_view.new_sub_view(LEASE_VIEW_PREFIX);
        let token_view = system_view.new_sub_view(TOKEN_VIEW_PREFIX);

        let expiration = ExpirationManager {
            self_ptr: Weak::new(),
            router: core.router.clone(),
            id_view: Arc::new(id_view),
            token_view: Arc::new(token_view),
            token_store: RwLock::new(Weak::new()),
            queue: Arc::new(RwLock::new(PriorityQueue::new())),
        };

        Ok(expiration)
    }

    /// Wraps the ExpirationManager in an Arc and sets the weak pointer.
    pub fn wrap(self) -> Arc<Self> {
        let mut wrap_self = Arc::new(self);
        let weak_self = Arc::downgrade(&wrap_self);
        unsafe {
            let ptr_self = Arc::into_raw(wrap_self) as *mut Self;
            (*ptr_self).self_ptr = weak_self;
            wrap_self = Arc::from_raw(ptr_self);
        }

        wrap_self
    }

    /// Sets the token store for the ExpirationManager.
    pub fn set_token_store(&self, ts: &Arc<TokenStore>) -> Result<(), RvError> {
        let mut token_store = self.token_store.write()?;
        *token_store = Arc::downgrade(ts);
        Ok(())
    }

    /// Restores the lease entries from the storage.
    #[maybe_async::maybe_async]
    pub async fn restore(&self) -> Result<(), RvError> {
        let existing = self.id_view.get_keys().await?;

        for lease_id in existing {
            let le = self.load_lease_entry(&lease_id).await?;
            if le.is_none() {
                continue;
            }

            self.register_lease_entry(Arc::new(le.unwrap()))?;
        }

        Ok(())
    }

    /// Renews a lease entry by the given increment.
    #[maybe_async::maybe_async]
    pub async fn renew(&self, lease_id: &str, increment: Duration) -> Result<Option<Response>, RvError> {
        let le = self.load_lease_entry(lease_id).await?;
        if le.is_none() {
            return Err(RvError::ErrLeaseNotFound);
        }

        let mut le = le.unwrap();

        if !le.renewable() {
            return Err(RvError::ErrLeaseNotRenewable);
        }

        let resp = self.renew_secret_lease_entry(&le, increment).await?;
        if resp.is_none() {
            return Ok(None);
        }

        let mut resp = resp.unwrap();
        if resp.secret.is_none() || !resp.secret.as_ref().unwrap().enabled() {
            return Ok(Some(resp));
        }

        if let Some(secret) = resp.secret.as_mut() {
            secret.ttl = calculate_ttl(
                MAX_LEASE_TTL,
                DEFAULT_LEASE_TTL,
                increment,
                Duration::ZERO,
                secret.ttl,
                secret.max_ttl,
                Duration::ZERO,
                le.issue_time,
            )?;
            secret.lease_id = lease_id.into();
        }

        le.data = resp.data.clone().unwrap_or(Map::new());
        le.expire_time = resp.secret.as_ref().unwrap().expiration_time();
        le.secret.clone_from(&resp.secret);

        self.persist_lease_entry(&le).await?;
        self.register_lease_entry(Arc::new(le))?;

        Ok(Some(resp))
    }

    /// Renews a token by the given increment.
    #[maybe_async::maybe_async]
    pub async fn renew_token(
        &self,
        req: &mut Request,
        te: &TokenEntry,
        increment: Duration,
    ) -> Result<Option<Response>, RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let lease_id = format!("{}/{}", te.path, token_store.salt_id(&te.id));

        let le = self.load_lease_entry(&lease_id).await?;
        if le.is_none() {
            return Err(RvError::ErrLeaseNotFound);
        }

        let mut le = le.unwrap();

        if !le.renewable() {
            return Err(RvError::ErrLeaseNotRenewable);
        }

        let resp = self.renew_auth_lease_entry(req, &le, increment).await?;
        if resp.is_none() {
            return Ok(None);
        }

        let resp = resp.unwrap();
        if resp.auth.is_none() {
            return Ok(None);
        }

        let mut auth = resp.auth.unwrap();

        auth.ttl = calculate_ttl(
            MAX_LEASE_TTL,
            DEFAULT_LEASE_TTL,
            increment,
            auth.period,
            auth.ttl,
            auth.max_ttl,
            auth.explicit_max_ttl,
            le.issue_time,
        )?;
        auth.client_token.clone_from(&te.id);

        le.expire_time = auth.expiration_time();
        le.auth = Some(auth.clone());

        self.persist_lease_entry(&le).await?;
        self.register_lease_entry(Arc::new(le))?;

        Ok(Some(Response { auth: Some(auth), ..Response::default() }))
    }

    /// Registers a secret from a response for lease management.
    #[maybe_async::maybe_async]
    pub async fn register_secret(&self, req: &mut Request, resp: &mut Response) -> Result<String, RvError> {
        if let Some(secret) = resp.secret.as_mut() {
            if secret.ttl.as_secs() == 0 {
                secret.ttl = DEFAULT_LEASE_DURATION_SECS;
            }

            if secret.ttl > MAX_LEASE_DURATION_SECS {
                secret.ttl = MAX_LEASE_DURATION_SECS;
            }

            let now = SystemTime::now();
            secret.issue_time = Some(now);

            let lease_id = format!("{}/{}", req.path, generate_uuid());

            secret.lease_id.clone_from(&lease_id);

            let le = LeaseEntry {
                lease_id: lease_id.clone(),
                client_token: req.client_token.clone(),
                path: req.path.clone(),
                data: resp.data.clone().unwrap_or_default(),
                secret: Some(secret.clone()),
                issue_time: now,
                expire_time: secret.expiration_time(),
                ..Default::default()
            };

            self.persist_lease_entry(&le).await?;
            self.create_index_by_token(&le.client_token, &le.lease_id).await?;

            secret.ttl = le.expire_time.duration_since(now)?;

            self.register_lease_entry(Arc::new(le))?;

            return Ok(lease_id);
        }

        Ok("".into())
    }

    /// Registers an authentication entry for lease management.
    #[maybe_async::maybe_async]
    pub async fn register_auth(&self, te: &TokenEntry, auth: &mut Auth) -> Result<(), RvError> {
        if te.ttl == 0
            && auth.expiration_time() == SystemTime::UNIX_EPOCH
            && (te.policies.len() != 1 || te.policies[0] != "root")
        {
            return Err(rv_error_string!("refusing to register a lease for a non-root token with no TTL"));
        }

        if auth.client_token.is_empty() {
            return Err(rv_error_string!("cannot register an auth lease with an empty token"));
        }

        if te.path.contains("..") {
            return Err(rv_error_string!(
                "cannot register an auth lease with n token entry whose path contains parent references"
            ));
        }

        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let lease_id = format!("{}/{}", te.path, token_store.salt_id(&auth.client_token));

        let now = SystemTime::now();
        auth.issue_time = Some(now);

        let le = LeaseEntry {
            lease_id,
            client_token: auth.client_token.clone(),
            path: te.path.clone(),
            auth: Some(auth.clone()),
            issue_time: now,
            expire_time: auth.expiration_time(),
            ..Default::default()
        };

        self.persist_lease_entry(&le).await?;
        self.register_lease_entry(Arc::new(le))?;

        Ok(())
    }

    /// Revokes a lease entry by its lease ID.
    #[maybe_async::maybe_async]
    pub async fn revoke_lease_id(&self, lease_id: &str, register_lease_entry: bool) -> Result<(), RvError> {
        let le = self.load_lease_entry(lease_id).await?;
        if le.is_none() {
            return Ok(());
        }

        let mut le = le.unwrap();

        log::debug!("revoke lease_id: {}", &le.lease_id);

        self.revoke_lease_entry(&le).await?;
        self.delete_lease_entry(lease_id).await?;

        if le.secret.is_some() {
            self.remove_index_by_token(&le.client_token, &le.lease_id).await?;
        }

        if register_lease_entry {
            le.expire_time = SystemTime::UNIX_EPOCH;
            self.register_lease_entry(Arc::new(le))?;
        }

        Ok(())
    }

    /// Revokes all lease entries with a given prefix.
    #[maybe_async::maybe_async]
    pub async fn revoke_prefix(&self, prefix: &str) -> Result<(), RvError> {
        let mut prefix = prefix.to_string();
        if !prefix.ends_with('/') {
            prefix += "/";
        }

        let sub = self.id_view.new_sub_view(&prefix);
        let existing = sub.get_keys().await?;
        for suffix in existing.iter() {
            let lease_id = format!("{prefix}{suffix}");
            self.revoke_lease_id(&lease_id, true).await?;
        }

        Ok(())
    }

    /// Revokes all lease entries associated with a given token.
    #[maybe_async::maybe_async]
    pub async fn revoke_by_token(&self, te: &TokenEntry) -> Result<(), RvError> {
        let existing = self.lookup_by_token(&te.id).await?;
        for lease_id in existing.iter() {
            self.revoke_lease_id(lease_id, true).await?;
        }

        Ok(())
    }

    /// Get the value of lease_count.
    pub fn get_lease_count(&self) -> usize {
        self.queue.read().map(|queue| queue.len()).unwrap_or(0)
    }

    /// Starts a background task to check for and handle expired lease entries.
    pub fn start_check_expired_lease_entries(&self) {
        let queue = self.queue.clone();
        let expiration = self.self_ptr.upgrade().unwrap().clone();

        let ticker = tick(Duration::from_millis(200));
        thread::spawn(move || {
            let rt = actix_rt::Runtime::new().unwrap();
            let queue_cloned = queue;
            let expiration_cloned = expiration;
            rt.block_on(async move {
                loop {
                    select! {
                        recv(ticker) -> _ => {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_millis()).unwrap_or(0);
                            let expired = {
                                let queue_locked = queue_cloned.read().unwrap();
                                queue_locked.peek().map(|(_le, Reverse(priority))| *priority < now).unwrap_or(false)
                            };

                            if !expired {
                                continue;
                            }

                            let mut queue_write_locked = queue_cloned.write().unwrap();
                            loop {
                                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_millis()).unwrap_or(0);
                                if let Some((le, Reverse(priority))) = queue_write_locked.peek() {
                                    if *priority > now {
                                        break;
                                    }

                                    if *priority != 0 {
                                        #[cfg(not(feature = "sync_handler"))]
                                        if let Err(e) = expiration_cloned.revoke_lease_id(&le.lease_id, false).await {
                                            log::warn!(
                                                "check_expired_lease_entries call revoke_lease_id err: {:?}, lease_id: {}, now: \
                                                {}, priority: {}, expire_time: {:?}",
                                                e,
                                                le.lease_id,
                                                now,
                                                *priority,
                                                le.expire_time
                                            );
                                            break;
                                        }
                                        #[cfg(feature = "sync_handler")]
                                        if let Err(e) = expiration_cloned.revoke_lease_id(&le.lease_id, false) {
                                            log::warn!(
                                                "check_expired_lease_entries call revoke_lease_id err: {:?}, lease_id: {}, now: \
                                                {}, priority: {}, expire_time: {:?}",
                                                e,
                                                le.lease_id,
                                                now,
                                                *priority,
                                                le.expire_time
                                            );
                                            break;
                                        }
                                    }
                                } else {
                                    break;
                                }

                                let _le = queue_write_locked.pop();
                            }
                        }
                    }
                }
            });
        });
    }

    /// Stops the background task that checks for expired lease entries.
    pub fn stop_check_expired_lease_entries(&self) -> Result<(), RvError> {
        let mut queue_write_locked = self.queue.write()?;
        queue_write_locked.clear();
        Ok(())
    }

    /// Registers a lease entry in the priority queue for expiration tracking.
    fn register_lease_entry(&self, le: Arc<LeaseEntry>) -> Result<(), RvError> {
        let priority = le.expire_time.duration_since(UNIX_EPOCH)?.as_millis();
        let mut queue_locked = self.queue.write()?;
        queue_locked.push(le, Reverse(priority));
        Ok(())
    }

    /// Loads a lease entry from storage by lease ID, updating if necessary from old to new format.
    #[maybe_async::maybe_async]
    async fn load_lease_entry(&self, lease_id: &str) -> Result<Option<LeaseEntry>, RvError> {
        let raw = self.id_view.get(lease_id).await?;
        if raw.is_none() {
            return Ok(None);
        }

        if let Ok(le) = serde_json::from_slice::<LeaseEntry>(raw.clone().unwrap().value.as_slice()) {
            return Ok(Some(le));
        }

        // Because the data field type of LeaseEntry has changed, it is necessary to convert OldLeaseEntry to LeaseEntry
        // and update the data in its storage.
        if let Ok(ole) = serde_json::from_slice::<OldLeaseEntry>(raw.unwrap().value.as_slice()) {
            let le = LeaseEntry {
                lease_id: ole.lease_id.clone(),
                client_token: ole.client_token.clone(),
                path: ole.path.clone(),
                data: ole.data.clone().map(|serde_map| serde_map.into_iter().collect()).unwrap_or(Map::new()),
                secret: ole.secret.clone(),
                auth: ole.auth.clone(),
                issue_time: ole.issue_time,
                expire_time: ole.expire_time,
                ..Default::default()
            };
            self.persist_lease_entry(&le).await?;
            return Ok(Some(le));
        }

        Ok(None)
    }

    /// Persists a lease entry to storage.
    #[maybe_async::maybe_async]
    async fn persist_lease_entry(&self, le: &LeaseEntry) -> Result<(), RvError> {
        let value = serde_json::to_string(&le)?;

        let entry = StorageEntry { key: le.lease_id.clone(), value: value.as_bytes().to_vec() };

        self.id_view.put(&entry).await
    }

    /// Deletes a lease entry from storage.
    #[maybe_async::maybe_async]
    async fn delete_lease_entry(&self, lease_id: &str) -> Result<(), RvError> {
        self.id_view.delete(lease_id).await
    }

    /// Creates an index in the token view using the provided token and lease ID.
    #[maybe_async::maybe_async]
    async fn create_index_by_token(&self, token: &str, lease_id: &str) -> Result<(), RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let key = format!("{}/{}", token_store.salt_id(token), token_store.salt_id(lease_id));
        let entry = StorageEntry { key, value: lease_id.as_bytes().to_owned() };
        self.token_view.put(&entry).await
    }

    /// Removes an index from the token view based on the provided token and lease ID.
    #[maybe_async::maybe_async]
    async fn remove_index_by_token(&self, token: &str, lease_id: &str) -> Result<(), RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let key = format!("{}/{}", token_store.salt_id(token), token_store.salt_id(lease_id));
        self.token_view.delete(&key).await
    }

    /// Retrieves an index from the token view based on the provided token and lease ID.
    #[allow(dead_code)]
    #[maybe_async::maybe_async]
    async fn index_by_token(&self, token: &str, lease_id: &str) -> Result<Option<StorageEntry>, RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let key = format!("{}/{}", token_store.salt_id(token), token_store.salt_id(lease_id));
        self.token_view.get(&key).await
    }

    /// Looks up lease entries associated with a specific token.
    #[maybe_async::maybe_async]
    async fn lookup_by_token(&self, token: &str) -> Result<Vec<String>, RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;
        let prefix = format!("{}/", token_store.salt_id(token));
        let sub_keys = self.token_view.list(&prefix).await?;

        let mut ret: Vec<String> = Vec::new();

        for sub in sub_keys.iter() {
            let key = format!("{prefix}{sub}");
            let raw = self.token_view.get(&key).await?;
            if raw.is_none() {
                continue;
            }

            let lease_id = String::from_utf8_lossy(&raw.unwrap().value).to_string();
            ret.push(lease_id);
        }

        Ok(ret)
    }

    /// Revokes a lease entry and handles secret or token revocation.
    #[maybe_async::maybe_async]
    async fn revoke_lease_entry(&self, le: &LeaseEntry) -> Result<(), RvError> {
        let token_store = self.token_store.read()?.upgrade().ok_or(RvError::ErrBarrierSealed)?;

        if le.auth.is_some() {
            return token_store.revoke_tree(&le.auth.as_ref().unwrap().client_token).await;
        }

        let mut secret: Option<SecretData> = None;
        if le.secret.is_some() {
            secret = Some(le.secret.as_ref().unwrap().clone());
        }

        let mut data: Option<Map<String, Value>> = None;
        if !le.data.is_empty() {
            data = Some(le.data.clone());
        }

        let mut req = Request::new_revoke_request(&le.path, secret, data);
        if let Err(e) = self.router.handle_request(&mut req).await {
            log::error!("failed to revoke entry: {le:?}, err: {e}");
        }

        Ok(())
    }

    /// Renews a secret lease entry with a specified increment duration.
    #[maybe_async::maybe_async]
    async fn renew_secret_lease_entry(
        &self,
        le: &LeaseEntry,
        increment: Duration,
    ) -> Result<Option<Response>, RvError> {
        let mut secret: Option<SecretData> = None;
        if le.secret.is_some() {
            let mut s = le.secret.as_ref().unwrap().clone();
            s.lease_id = "".to_string();
            s.increment = increment;
            s.issue_time = Some(le.issue_time);
            secret = Some(s);
        }

        let mut data: Option<Map<String, Value>> = None;
        if !le.data.is_empty() {
            data = Some(le.data.clone());
        }

        let mut req = Request::new_renew_request(&le.path, secret, data);
        let ret = self.router.handle_request(&mut req).await;
        if ret.is_err() {
            log::error!("failed to renew entry: {}", ret.as_ref().unwrap_err());
        }

        ret
    }

    /// Renews an authentication lease entry with a specified increment duration.
    #[maybe_async::maybe_async]
    async fn renew_auth_lease_entry(
        &self,
        _req: &mut Request,
        le: &LeaseEntry,
        increment: Duration,
    ) -> Result<Option<Response>, RvError> {
        let mut auth: Option<Auth> = None;
        if le.auth.is_some() {
            let mut au = le.auth.as_ref().unwrap().clone();
            if le.path.starts_with("auth/token/") {
                au.client_token.clone_from(&le.client_token);
            } else {
                au.client_token = "".to_string();
            }
            au.increment = increment;
            au.issue_time = Some(le.issue_time);
            auth = Some(au);
        }

        let mut req = Request::new_renew_auth_request(&le.path, auth, None);
        let ret = self.router.handle_request(&mut req).await;
        if ret.is_err() {
            log::error!("failed to renew_auth entry: {}", ret.as_ref().unwrap_err());
        }

        ret
    }
}

#[cfg(test)]
mod mod_expiration_tests {
    use std::{sync::Mutex, thread::sleep};

    use serde_json::json;

    use super::*;
    use crate::{
        context::Context,
        logical::{Backend, Field, FieldType, Lease, LogicalBackend, Operation, Path, PathOperation, Secret},
        mount::{MountEntry, MOUNT_TABLE_TYPE},
        new_fields, new_fields_internal, new_logical_backend, new_logical_backend_internal, new_path,
        new_path_internal, new_secret, new_secret_internal,
        test_utils::{new_unseal_test_rusty_vault, NoopBackend},
    };

    macro_rules! mock_expiration_manager {
        () => {{
            let name = format!("{}_{}", file!(), line!()).replace("/", "_").replace("\\", "_").replace(".", "_");
            println!("init_test_rusty_vault, name: {}", name);
            #[cfg(not(feature = "sync_handler"))]
            let (_, core, _) = new_unseal_test_rusty_vault(&name).await;
            #[cfg(feature = "sync_handler")]
            let (_, core, _) = new_unseal_test_rusty_vault(&name);

            let expiration = ExpirationManager::new(&core).unwrap().wrap();
            #[cfg(not(feature = "sync_handler"))]
            let token_store = TokenStore::new(&core, expiration.clone()).await.unwrap().wrap();
            #[cfg(feature = "sync_handler")]
            let token_store = TokenStore::new(&core, expiration.clone()).unwrap().wrap();

            expiration.set_token_store(&token_store).unwrap();
            #[cfg(not(feature = "sync_handler"))]
            expiration.restore().await.unwrap();
            #[cfg(feature = "sync_handler")]
            expiration.restore().unwrap();
            expiration.start_check_expired_lease_entries();
            (core, expiration, token_store)
        }};
    }

    #[maybe_async::maybe_async]
    pub async fn renew_noop_handler(_backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[maybe_async::maybe_async]
    pub async fn revoke_noop_handler(_backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[actix_rt::test]
    async fn test_secret_expiration() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let new_now: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
        let new_now_cloned = new_now.clone();
        let renew_flag = Arc::new(Mutex::new(false));
        let renew_flag_cloned = renew_flag.clone();

        let secret = Arc::new(Secret {
            secret_type: "test".into(),
            default_duration: Duration::from_secs(5),
            renew_handler: Some(Arc::new(
                #[cfg(not(feature = "sync_handler"))]
                move |_backend: &dyn Backend, req: &mut Request| {
                    let renew_flag_cloned = renew_flag_cloned.clone();
                    Box::pin(async move {
                        //let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
                        let mut renew_flag_cloned_locked = renew_flag_cloned.lock().unwrap();
                        *renew_flag_cloned_locked = true;

                        let mut resp = Response::default();
                        resp.data = req.data.clone();
                        resp.secret = req.secret.clone();
                        Ok(Some(resp))
                    })
                },
                #[cfg(feature = "sync_handler")]
                move |_backend: &dyn Backend, req: &mut Request| -> Result<Option<Response>, RvError> {
                    //let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
                    let mut renew_flag_cloned_locked = renew_flag_cloned.lock().unwrap();
                    *renew_flag_cloned_locked = true;

                    let mut resp = Response::default();
                    resp.data = req.data.clone();
                    resp.secret = req.secret.clone();
                    Ok(Some(resp))
                },
            )),
            revoke_handler: Some(Arc::new(
                #[cfg(not(feature = "sync_handler"))]
                move |_backend: &dyn Backend, _req: &mut Request| {
                    let new_now_cloned = new_now_cloned.clone();
                    Box::pin(async move {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
                        let mut new_now_cloned_locked = new_now_cloned.lock().unwrap();
                        *new_now_cloned_locked = now;
                        Ok(None)
                    })
                },
                #[cfg(feature = "sync_handler")]
                move |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
                    let mut new_now_cloned_locked = new_now_cloned.lock().unwrap();
                    *new_now_cloned_locked = now;
                    Ok(None)
                },
            )),
        });
        let secret_cloned = secret.clone();

        #[cfg(not(feature = "sync_handler"))]
        let new_backend_fn = move || -> LogicalBackend {
            let mut mock_logical_backend = new_logical_backend!({
                paths: [
                    {
                        pattern: "/(?P<bar>.+?)",
                        fields: {
                            "mytype": {
                                field_type: FieldType::Int,
                                description: "haha"
                            },
                            "mypath": {
                                field_type: FieldType::Str,
                                description: "hehe"
                            },
                            "mypassword": {
                                field_type: FieldType::SecretStr,
                                description: "password"
                            }
                        },
                        operations: [
                            {op: Operation::Read, raw_handler: async move |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError>
                                {
                                    Ok(None)
                                }
                            },
                            {op: Operation::Write, raw_handler: async move |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                                    Ok(Some(Response::new()))
                                }
                            },
                            {op: Operation::Delete, raw_handler: async move |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                                    Err(RvError::ErrUnknown)
                                }
                            }
                        ]
                    }
                ],
                secrets: [{
                    secret_type: "kv",
                    default_duration: 60,
                    renew_handler: renew_noop_handler,
                    revoke_handler: revoke_noop_handler,
                }],
                unauth_paths: ["/login"],
                root_paths: ["/"],
                help: "help content",
            });

            mock_logical_backend.secrets.push(secret.clone());

            mock_logical_backend
        };
        #[cfg(feature = "sync_handler")]
        let new_backend_fn = move || -> LogicalBackend {
            let mut mock_logical_backend = new_logical_backend!({
                paths: [
                    {
                        pattern: "/(?P<bar>.+?)",
                        fields: {
                            "mytype": {
                                field_type: FieldType::Int,
                                description: "haha"
                            },
                            "mypath": {
                                field_type: FieldType::Str,
                                description: "hehe"
                            },
                            "mypassword": {
                                field_type: FieldType::SecretStr,
                                description: "password"
                            }
                        },
                        operations: [
                            {op: Operation::Read, raw_handler: |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError>
                                {
                                    Ok(None)
                                }
                            },
                            {op: Operation::Write, raw_handler: |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                                    Ok(Some(Response::new()))
                                }
                            },
                            {op: Operation::Delete, raw_handler: |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                                    Err(RvError::ErrUnknown)
                                }
                            }
                        ]
                    }
                ],
                secrets: [{
                    secret_type: "kv",
                    default_duration: 60,
                    renew_handler: renew_noop_handler,
                    revoke_handler: revoke_noop_handler,
                }],
                unauth_paths: ["/login"],
                root_paths: ["/"],
                help: "help content",
            });

            mock_logical_backend.secrets.push(secret.clone());

            mock_logical_backend
        };

        core.add_logical_backend(
            "test",
            Arc::new(move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
                let mut test_backend = new_backend_fn();
                test_backend.init()?;
                Ok(Arc::new(test_backend))
            }),
        )
        .unwrap();

        let me = MountEntry::new(MOUNT_TABLE_TYPE, "mytest/", "test", "test description");
        #[cfg(not(feature = "sync_handler"))]
        core.mount(&me).await.unwrap();
        #[cfg(feature = "sync_handler")]
        core.mount(&me).unwrap();

        let mut request = Request::new("mytest/tt");
        request.client_token = "mytest".into();

        let mut response = secret_cloned.response(json!({"key": "test"}).as_object().cloned(), None);

        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);

        // register secret
        #[cfg(not(feature = "sync_handler"))]
        let result = expiration.register_secret(&mut request, &mut response).await;
        #[cfg(feature = "sync_handler")]
        let result = expiration.register_secret(&mut request, &mut response);
        assert!(result.is_ok());

        println!("sleep 10s");
        sleep(Duration::from_secs(10));

        {
            // Check if the `revoke_handler` callback is called when `secret` expires. In `revoke_handler`,
            // we set external variable `new_now` to the current time. `new_now` minus the current time gives the `secret`'s TTL.
            let new_now_locked = new_now.lock().unwrap();
            assert_ne!(*new_now_locked, 0);
            assert!(*new_now_locked - now >= 5);
            assert!(*new_now_locked - now <= 6);
        }

        // register secret again
        //let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);
        #[cfg(not(feature = "sync_handler"))]
        let result = expiration.register_secret(&mut request, &mut response).await;
        #[cfg(feature = "sync_handler")]
        let result = expiration.register_secret(&mut request, &mut response);
        assert!(result.is_ok());

        // test renew
        let lease_id = response.secret.as_ref().unwrap().lease_id.clone();
        #[cfg(not(feature = "sync_handler"))]
        let result = expiration.renew(&lease_id, Duration::from_secs(3)).await;
        #[cfg(feature = "sync_handler")]
        let result = expiration.renew(&lease_id, Duration::from_secs(3));
        assert!(result.is_ok());
        let renew_flag_locked = renew_flag.lock().unwrap();
        assert_eq!(*renew_flag_locked, true);

        println!("sleep 10s");
        sleep(Duration::from_secs(10));
        {
            // TODO: The secret's initial TTL is 5s. After a 3s increase during renewal, the TTL should be 8s.
            let new_now_locked = new_now.lock().unwrap();
            assert_ne!(*new_now_locked, 0);
        }
    }

    #[test]
    fn test_auth_expiration() {
        // TODO
        println!("TODO");
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_persist_and_load_lease_entry() {
        let (_core, expiration, _token_store) = mock_expiration_manager!();

        let le = LeaseEntry {
            lease_id: generate_uuid(),
            client_token: generate_uuid(),
            path: "test/kk".into(),
            expire_time: SystemTime::now() + Duration::from_secs(3600), // 1 hour from now
            ..Default::default()
        };

        expiration.persist_lease_entry(&le).await.unwrap();
        let loaded = expiration.load_lease_entry(&le.lease_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded, Some(le));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_total_lease_count() {
        let (_core, expiration, _token_store) = mock_expiration_manager!();

        let n: usize = 100;
        for i in 0..n {
            let le = LeaseEntry {
                lease_id: format!("lease-{}", i),
                client_token: format!("client_token-{}", i),
                path: format!("foo/bar/{}", i),
                expire_time: SystemTime::now() + Duration::from_secs(3600), // 1 hour from now
                ..Default::default()
            };

            let result = expiration.persist_lease_entry(&le).await;
            assert!(result.is_ok());
            let result = expiration.register_lease_entry(Arc::new(le));
            assert!(result.is_ok());
        }

        let lease_count = expiration.get_lease_count();
        assert_eq!(n, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        assert_eq!(keys.unwrap().len(), n);

        let m: usize = 20;
        for i in 0..m {
            let le = LeaseEntry {
                lease_id: format!("new-lease-{}", i),
                client_token: format!("new-client_token-{}", i),
                path: format!("new/foo/bar/{}", i),
                expire_time: SystemTime::now() + Duration::from_secs(5), // 5s from now
                ..Default::default()
            };

            let result = expiration.persist_lease_entry(&le).await;
            assert!(result.is_ok());
            assert!(expiration.register_lease_entry(Arc::new(le)).is_ok());
        }

        let lease_count = expiration.get_lease_count();
        assert_eq!(m + n, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        assert_eq!(keys.unwrap().len(), m + n);

        println!("sleep 7s");
        sleep(Duration::from_secs(7));

        let lease_count = expiration.get_lease_count();
        assert_eq!(n, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        assert_eq!(keys.unwrap().len(), n);

        let m: usize = 30;
        for i in 0..m {
            let le = LeaseEntry {
                lease_id: format!("lease-{}", i),
                client_token: format!("client_token-{}", i),
                path: format!("foo/bar/{}", i),
                expire_time: SystemTime::now() + Duration::from_secs(5), // Marked as expiring in 5 seconds.
                ..Default::default()
            };

            let result = expiration.persist_lease_entry(&le).await;
            assert!(result.is_ok());
            assert!(expiration.register_lease_entry(Arc::new(le)).is_ok());
        }

        println!("sleep 6s");
        sleep(Duration::from_secs(6));

        let lease_count = expiration.get_lease_count();
        assert_eq!(n - m, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        assert_eq!(keys.unwrap().len(), n - m);

        let k: usize = 30;
        for i in 50..(50 + k) {
            let lease_id = format!("lease-{}", i);

            let result = expiration.revoke_lease_id(&lease_id, true).await;
            assert!(result.is_ok());
        }

        println!("sleep 1s");
        sleep(Duration::from_secs(1));

        let lease_count = expiration.get_lease_count();
        assert_eq!(n - m - k, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        assert_eq!(keys.unwrap().len(), n - m - k);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_register_and_restore_benchmark() {
        let (_core, expiration, _token_store) = mock_expiration_manager!();

        let n = 10000;
        for i in 0..n {
            let mut secret = SecretData::default();
            secret.ttl = Duration::from_secs(400);
            let mut request = Request::new(&format!("secret/{}", i));
            request.client_token = "root".into();

            let mut response = Response {
                secret: Some(secret),
                data: json!({"access_key": "xyz", "secret_key": "abc"}).as_object().cloned(),
                ..Default::default()
            };

            // register secret
            let result = expiration.register_secret(&mut request, &mut response).await;
            assert!(result.is_ok());
        }

        println!("sleep 5s");
        sleep(Duration::from_secs(5));

        assert!(expiration.stop_check_expired_lease_entries().is_ok());

        let result = expiration.restore().await;
        assert!(result.is_ok());

        let lease_count = expiration.get_lease_count();
        assert_eq!(n, lease_count);

        let keys = expiration.id_view.get_keys().await;
        assert!(keys.is_ok());
        let lease_id_vec = keys.unwrap();
        assert_eq!(lease_id_vec.len(), n);
        let mut lease_id_prefix_vec: Vec<String> = Vec::with_capacity(n);
        for i in 0..lease_id_vec.len() {
            lease_id_prefix_vec.push(format!("secret/{}/", i));
        }
        lease_id_prefix_vec.sort();
        for i in 0..lease_id_vec.len() {
            assert!(lease_id_vec[i].starts_with(&lease_id_prefix_vec[i]));
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_register_auth() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let mut auth = Auth { client_token: root.id.clone(), ..Default::default() };
        auth.ttl = Duration::from_secs(60 * 60);

        let te = TokenEntry { path: "auth/github/login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());

        let te = TokenEntry { path: "auth/github/../login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_register_auth_no_lease() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let mut auth = Auth { client_token: root.id.clone(), ..Default::default() };

        let mut te = TokenEntry {
            id: root.id.clone(),
            path: "auth/github/login".into(),
            policies: vec!["root".into()],
            ..Default::default()
        };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());

        let mut request = Request::new("");

        // Should not be able to renew, no expiration
        let resp = expiration.renew_token(&mut request, &te, Duration::ZERO).await;
        assert_eq!(resp.unwrap_err(), RvError::ErrLeaseNotRenewable);

        // Wait and check token is not invalidated
        println!("sleep 2s");
        sleep(Duration::from_secs(2));

        let ret = token_store.lookup(&root.id).await;
        assert!(ret.is_ok());
        assert!(ret.unwrap().is_some());

        te.policies[0] = "default".into();
        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend::default());
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(60 * 60), ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let result = expiration.revoke_lease_id(&id, true).await;
        assert!(result.is_ok());

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Revoke);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke_on_expire() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend::default());
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(1), ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());

        sleep(Duration::from_millis(1500));

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Revoke);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke_prefix() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend::default());
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let paths = ["prod/aws/foo", "prod/aws/sub/bar", "prod/aws/zip"];

        for path in paths.iter() {
            let mut req = Request::new(*path);
            req.client_token = "foobar".into();
            let mut resp = Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(1), ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "xyz",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            };

            let ret = expiration.register_secret(&mut req, &mut resp).await;
            assert!(ret.is_ok());
        }

        let result = expiration.revoke_prefix("prod/aws/").await;
        assert!(result.is_ok());

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 3);

        for r in req.iter() {
            assert_eq!(r.operation, Operation::Revoke);
        }

        let noop_paths = noop.paths.read().unwrap();
        let mut paths = noop_paths.clone();
        paths.sort();
        let mut expect = vec!["foo".to_string(), "sub/bar".to_string(), "zip".to_string()];
        expect.sort();
        assert_eq!(paths, expect);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke_by_token() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend::default());
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let paths = ["prod/aws/foo", "prod/aws/sub/bar", "prod/aws/zip"];

        for path in paths.iter() {
            let mut req = Request::new(*path);
            req.client_token = "foobar".into();
            let mut resp = Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(1), ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "xyz",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            };

            let ret = expiration.register_secret(&mut req, &mut resp).await;
            assert!(ret.is_ok());
        }

        let te = TokenEntry { id: "foobar".into(), ..Default::default() };

        let result = expiration.revoke_by_token(&te).await;
        assert!(result.is_ok());

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 3);

        for r in req.iter() {
            assert_eq!(r.operation, Operation::Revoke);
        }

        let noop_paths = noop.paths.read().unwrap();
        let mut paths = noop_paths.clone();
        paths.sort();
        let mut expect = vec!["foo".to_string(), "sub/bar".to_string(), "zip".to_string()];
        expect.sort();
        assert_eq!(paths, expect);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_token() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let mut auth = Auth {
            client_token: root.id.clone(),
            lease: Lease { ttl: Duration::from_secs(60 * 60), renewable: true, ..Default::default() },
            ..Default::default()
        };

        let te = TokenEntry { id: root.id, path: "auth/token/login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());

        let mut req = Request::default();

        let resp = expiration.renew_token(&mut req, &te, Duration::from_secs(0)).await.unwrap();
        assert!(resp.is_some());
        let resp_auth = resp.unwrap().auth;
        assert!(resp_auth.is_some());
        assert_eq!(auth.client_token, resp_auth.unwrap().client_token);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_token_period() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let mut root = TokenEntry {
            policies: vec!["root".to_string()],
            path: "auth/token/root".into(),
            display_name: "root".into(),
            creation_time: SystemTime::now(),
            period: Duration::from_secs(60),
            ..Default::default()
        };

        let result = token_store.create(&mut root).await;
        assert!(result.is_ok());

        let mut auth = Auth {
            client_token: root.id.clone(),
            lease: Lease { ttl: Duration::from_secs(60 * 60), renewable: true, ..Default::default() },
            period: Duration::from_secs(60),
            ..Default::default()
        };

        let te = TokenEntry { id: root.id, path: "auth/token/login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());
        assert_eq!(expiration.get_lease_count(), 1);

        let mut req = Request::default();

        let resp = expiration.renew_token(&mut req, &te, Duration::from_secs(0)).await.unwrap();
        assert!(resp.is_some());
        let resp_auth = resp.unwrap().auth;
        assert!(resp_auth.is_some());
        let auth = resp_auth.unwrap();
        assert_eq!(auth.client_token, auth.client_token);
        assert!(auth.ttl <= Duration::from_secs(60));

        assert_eq!(expiration.get_lease_count(), 1);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_token_period_backend() {
        let (core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                auth: Some(Auth {
                    lease: Lease { ttl: Duration::from_secs(10), renewable: true, ..Default::default() },
                    period: Duration::from_secs(5),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            default_lease_ttl: Duration::from_secs(5),
            max_lease_ttl: Duration::from_secs(5),
            ..Default::default()
        });
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "auth/foo/",
                Arc::new(RwLock::new(MountEntry {
                    path: "auth/foo/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut auth = Auth {
            client_token: root.id.clone(),
            lease: Lease { ttl: Duration::from_secs(10), renewable: true, ..Default::default() },
            period: Duration::from_secs(5),
            ..Default::default()
        };

        let te = TokenEntry { id: root.id, path: "auth/foo/login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());

        sleep(Duration::from_secs(3));

        let mut req = Request::default();

        let resp = expiration.renew_token(&mut req, &te, Duration::from_secs(0)).await.unwrap();
        assert!(resp.is_some());
        let resp_auth = resp.unwrap().auth;
        assert!(resp_auth.is_some());
        let auth = resp_auth.unwrap();
        assert!(auth.ttl != Duration::ZERO);
        assert!(auth.ttl <= Duration::from_secs(5));

        sleep(Duration::from_secs(3));

        let mut req = Request::default();

        let resp = expiration.renew_token(&mut req, &te, Duration::from_secs(0)).await.unwrap();
        assert!(resp.is_some());
        let resp_auth = resp.unwrap().auth;
        assert!(resp_auth.is_some());
        let auth = resp_auth.unwrap();
        assert!(auth.ttl >= Duration::from_secs(4));
        assert!(auth.ttl <= Duration::from_secs(5));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_token_not_renewable() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let mut auth = Auth {
            client_token: root.id.clone(),
            lease: Lease { ttl: Duration::from_secs(60 * 60), renewable: false, ..Default::default() },
            ..Default::default()
        };

        let te = TokenEntry { id: root.id.clone(), path: "auth/foo/login".into(), ..Default::default() };

        let result = expiration.register_auth(&te, &mut auth).await;
        assert!(result.is_ok());

        let mut req = Request::default();

        let te = TokenEntry { id: root.id, path: "auth/foo/login".into(), ..Default::default() };

        let resp = expiration.renew_token(&mut req, &te, Duration::from_secs(0)).await;
        assert!(resp.is_err());
        assert_eq!(resp.unwrap_err(), RvError::ErrLeaseNotRenewable);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(10), ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "123",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(1), renewable: true, ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let mut resp = expiration.renew(&id, Duration::ZERO).await.unwrap().unwrap();
        let mut secret = resp.secret.clone().unwrap();
        secret.lease_id.clear();
        resp.secret = Some(secret);
        assert_eq!(Some(resp), noop.response);

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Renew);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_not_renewable() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend::default());

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(1), renewable: false, ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let resp = expiration.renew(&id, Duration::ZERO).await;
        assert!(resp.is_err());
        assert_eq!(resp.unwrap_err(), RvError::ErrLeaseNotRenewable);

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 0);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_revoke_on_expire() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(1), ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "123",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(1), renewable: true, ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let resp = expiration.renew(&id, Duration::ZERO).await;
        assert!(resp.is_ok());

        sleep(Duration::from_millis(1500));

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 2);
        assert_eq!(req[1].operation, Operation::Revoke);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_final_second() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(2), max_ttl: Duration::from_secs(2), ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "123",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(2), renewable: true, ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let mut le = expiration.load_lease_entry(&id).await.unwrap().unwrap();
        le.auth = Some(Auth { lease: Lease { renewable: true, ..Default::default() }, ..Default::default() });

        let result = expiration.persist_lease_entry(&le).await;
        assert!(result.is_ok());

        sleep(Duration::from_millis(500));

        let resp = expiration.renew(&id, Duration::ZERO).await;
        assert!(resp.is_ok());

        assert_eq!(expiration.get_lease_count(), 1);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_final_second_lease() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend::default());

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "prod/aws/",
                Arc::new(RwLock::new(MountEntry {
                    path: "prod/aws/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut req = Request::new("prod/aws/foo");
        req.client_token = "foobar".into();
        let mut resp = Response {
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(2), renewable: true, ..Default::default() },
                ..Default::default()
            }),
            data: json!({
                "access_key": "xyz",
                "secret_key": "abcd",
            })
            .as_object()
            .cloned(),
            ..Default::default()
        };

        let ret = expiration.register_secret(&mut req, &mut resp).await;
        assert!(ret.is_ok());
        let id = ret.unwrap();

        let mut le = expiration.load_lease_entry(&id).await.unwrap().unwrap();
        le.auth = Some(Auth { lease: Lease { renewable: true, ..Default::default() }, ..Default::default() });

        let result = expiration.persist_lease_entry(&le).await;
        assert!(result.is_ok());

        sleep(Duration::from_secs(1));

        let resp = expiration.renew(&id, Duration::ZERO).await;
        assert!(resp.is_ok());

        assert_eq!(expiration.get_lease_count(), 1);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke_entry() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let noop = Arc::new(NoopBackend::default());
        let me_uuid = generate_uuid();

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "foo/bar/",
                Arc::new(RwLock::new(MountEntry {
                    path: "foo/bar/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let le = LeaseEntry {
            lease_id: "foo/bar/1234".into(),
            path: "foo/bar/".into(),
            issue_time: SystemTime::now(),
            expire_time: SystemTime::now(),
            data: json!({
                "testing": true
            })
            .as_object()
            .unwrap()
            .clone(),
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(60), ..Default::default() },
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = expiration.revoke_lease_entry(&le).await;
        assert!(result.is_ok());

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Revoke);
        assert_eq!(req[0].data, Some(le.data));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_revoke_entry_token() {
        let (_core, expiration, token_store) = mock_expiration_manager!();
        let root = token_store.root_token().await.unwrap();

        let le = LeaseEntry {
            client_token: root.id.clone(),
            lease_id: "foo/bar/1234".into(),
            path: "foo/bar/".into(),
            issue_time: SystemTime::now(),
            expire_time: SystemTime::now() + Duration::from_secs(60),
            auth: Some(Auth {
                client_token: root.id.clone(),
                lease: Lease { ttl: Duration::from_secs(60), ..Default::default() },
                ..Default::default()
            }),
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(60), ..Default::default() },
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = expiration.persist_lease_entry(&le).await;
        assert!(result.is_ok());

        let result = expiration.create_index_by_token(&le.client_token, &le.lease_id).await;
        assert!(result.is_ok());

        let index_entry = expiration.index_by_token(&le.client_token, &le.lease_id).await.unwrap();
        assert!(index_entry.is_some());

        let result = expiration.revoke_lease_entry(&le).await;
        assert!(result.is_ok());

        let index_entry = expiration.index_by_token(&le.client_token, &le.lease_id).await.unwrap();
        assert!(index_entry.is_none());

        let te = token_store.lookup(&le.client_token).await;
        assert!(te.is_ok());
        assert!(te.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_entry() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "logical/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                secret: Some(SecretData {
                    lease: Lease { ttl: Duration::from_secs(60 * 60), renewable: true, ..Default::default() },
                    ..Default::default()
                }),
                data: json!({
                    "access_key": "123",
                    "secret_key": "abcd",
                })
                .as_object()
                .cloned(),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "foo/bar/",
                Arc::new(RwLock::new(MountEntry {
                    path: "foo/bar/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let le = LeaseEntry {
            lease_id: "foo/bar/1234".into(),
            path: "foo/bar/".into(),
            issue_time: SystemTime::now(),
            expire_time: SystemTime::now(),
            data: json!({
                "testing": true
            })
            .as_object()
            .unwrap()
            .clone(),
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(60), ..Default::default() },
                ..Default::default()
            }),
            ..Default::default()
        };

        let resp = expiration.renew_secret_lease_entry(&le, Duration::ZERO).await;
        assert!(resp.is_ok());

        let resp = resp.unwrap();
        assert!(resp.is_some());

        let mut resp = resp.unwrap();
        let mut secret = resp.secret.clone().unwrap();
        secret.lease_id.clear();
        resp.secret = Some(secret);
        assert_eq!(Some(resp), noop.response);

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Renew);
        assert_eq!(req[0].data, Some(le.data));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_renew_auth_entry() {
        let (core, expiration, _token_store) = mock_expiration_manager!();
        let view = BarrierView::new(core.barrier.clone(), "auth/");
        let me_uuid = generate_uuid();
        let noop = Arc::new(NoopBackend {
            response: Some(Response {
                auth: Some(Auth {
                    lease: Lease { ttl: Duration::from_secs(60 * 60), renewable: true, ..Default::default() },
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(expiration
            .router
            .mount(
                noop.clone(),
                "auth/foo/",
                Arc::new(RwLock::new(MountEntry {
                    path: "auth/foo/".into(),
                    logical_type: "noop".into(),
                    uuid: me_uuid.clone(),
                    ..Default::default()
                })),
                view
            )
            .is_ok());

        let mut le_auth_internal_data: HashMap<String, String> = HashMap::new();
        le_auth_internal_data.insert("MySecret".to_string(), "secret".to_string());
        let le = LeaseEntry {
            lease_id: "auth/foo/1234".into(),
            path: "auth/foo/login".into(),
            auth: Some(Auth {
                lease: Lease { ttl: Duration::from_secs(60), renewable: true, ..Default::default() },
                internal_data: le_auth_internal_data.clone(),
                ..Default::default()
            }),
            issue_time: SystemTime::now(),
            expire_time: SystemTime::now() + Duration::from_secs(60),
            ..Default::default()
        };

        let mut req = Request::new("auth/foo/login");
        let resp = expiration.renew_auth_lease_entry(&mut req, &le, Duration::ZERO).await;
        assert!(resp.is_ok());

        let resp = resp.unwrap();
        assert!(resp.is_some());

        let resp = resp.unwrap();
        assert_eq!(Some(resp), noop.response);

        let req = noop.requests.read().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0].operation, Operation::Renew);
        assert_eq!(req[0].path, "login");
        assert_eq!(req[0].auth.clone().unwrap().internal_data, le_auth_internal_data);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_expiration_persist_load_delete() {
        let (_core, expiration, _token_store) = mock_expiration_manager!();

        let le = LeaseEntry {
            lease_id: "foo/bar/1234".into(),
            path: "foo/bar".into(),
            data: json!({
                "testing": true
            })
            .as_object()
            .unwrap()
            .clone(),
            secret: Some(SecretData {
                lease: Lease { ttl: Duration::from_secs(60), ..Default::default() },
                ..Default::default()
            }),
            issue_time: SystemTime::now(),
            expire_time: SystemTime::now(),
            ..Default::default()
        };

        let result = expiration.persist_lease_entry(&le).await;
        assert!(result.is_ok());

        let out = expiration.load_lease_entry("foo/bar/1234").await;
        assert!(out.is_ok());

        let out = out.unwrap();
        assert!(out.is_some());

        let out = out.unwrap();
        assert_eq!(le, out);

        let result = expiration.delete_lease_entry("foo/bar/1234").await;
        assert!(result.is_ok());

        let out = expiration.load_lease_entry("foo/bar/1234").await;
        assert!(out.is_ok());

        let out = out.unwrap();
        assert!(out.is_none());
    }
}
