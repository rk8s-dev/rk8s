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
use tokio::runtime::Runtime;

use super::{TokenStore, token_store::TokenEntry};
use crate::{
    core::Core,
    errors::RvError,
    logical::{Auth, Request, Response, SecretData, lease::calculate_ttl},
    router::Router,
    rv_error_string,
    storage::{Storage, StorageEntry, barrier_view::BarrierView},
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
    #[serde(
        serialize_with = "serialize_system_time",
        deserialize_with = "deserialize_system_time"
    )]
    pub issue_time: SystemTime,
    #[default(SystemTime::now())]
    #[serde(
        serialize_with = "serialize_system_time",
        deserialize_with = "deserialize_system_time"
    )]
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
    #[serde(
        serialize_with = "serialize_system_time",
        deserialize_with = "deserialize_system_time"
    )]
    pub issue_time: SystemTime,
    #[default(SystemTime::UNIX_EPOCH)]
    #[serde(
        serialize_with = "serialize_system_time",
        deserialize_with = "deserialize_system_time"
    )]
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
        self.lease_id == other.lease_id
            && self.client_token == other.client_token
            && self.path == other.path
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
        self.auth.as_ref().map_or(false, |a| {
            a.enabled() && a.policies.len() == 1 && a.policies[0] == "root"
        })
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
    pub async fn renew(
        &self,
        lease_id: &str,
        increment: Duration,
    ) -> Result<Option<Response>, RvError> {
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
    pub async fn renew_token(
        &self,
        req: &mut Request,
        te: &TokenEntry,
        increment: Duration,
    ) -> Result<Option<Response>, RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
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

        Ok(Some(Response {
            auth: Some(auth),
            ..Response::default()
        }))
    }

    /// Registers a secret from a response for lease management.
    pub async fn register_secret(
        &self,
        req: &mut Request,
        resp: &mut Response,
    ) -> Result<String, RvError> {
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
            self.create_index_by_token(&le.client_token, &le.lease_id)
                .await?;

            secret.ttl = le.expire_time.duration_since(now)?;

            self.register_lease_entry(Arc::new(le))?;

            return Ok(lease_id);
        }

        Ok("".into())
    }

    /// Registers an authentication entry for lease management.
    pub async fn register_auth(&self, te: &TokenEntry, auth: &mut Auth) -> Result<(), RvError> {
        if te.ttl == 0
            && auth.expiration_time() == SystemTime::UNIX_EPOCH
            && (te.policies.len() != 1 || te.policies[0] != "root")
        {
            return Err(rv_error_string!(
                "refusing to register a lease for a non-root token with no TTL"
            ));
        }

        if auth.client_token.is_empty() {
            return Err(rv_error_string!(
                "cannot register an auth lease with an empty token"
            ));
        }

        if te.path.contains("..") {
            return Err(rv_error_string!(
                "cannot register an auth lease with a token entry whose path contains parent references"
            ));
        }

        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
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
    pub async fn revoke_lease_id(
        &self,
        lease_id: &str,
        register_lease_entry: bool,
    ) -> Result<(), RvError> {
        let le = self.load_lease_entry(lease_id).await?;
        if le.is_none() {
            return Ok(());
        }

        let mut le = le.unwrap();

        log::debug!("revoke lease_id: {}", &le.lease_id);

        self.revoke_lease_entry(&le).await?;
        self.delete_lease_entry(lease_id).await?;

        if le.secret.is_some() {
            self.remove_index_by_token(&le.client_token, &le.lease_id)
                .await?;
        }

        if register_lease_entry {
            le.expire_time = SystemTime::UNIX_EPOCH;
            self.register_lease_entry(Arc::new(le))?;
        }

        Ok(())
    }

    /// Revokes all lease entries with a given prefix.
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
            let rt = Runtime::new().unwrap();
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

                                    if *priority != 0
                                        && let Err(e) = expiration_cloned
                                            .revoke_lease_id(&le.lease_id, false)
                                            .await
                                        {
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
    async fn load_lease_entry(&self, lease_id: &str) -> Result<Option<LeaseEntry>, RvError> {
        let raw = self.id_view.get(lease_id).await?;
        if raw.is_none() {
            return Ok(None);
        }

        if let Ok(le) = serde_json::from_slice::<LeaseEntry>(raw.clone().unwrap().value.as_slice())
        {
            return Ok(Some(le));
        }

        // Because the data field type of LeaseEntry has changed, it is necessary to convert OldLeaseEntry to LeaseEntry
        // and update the data in its storage.
        if let Ok(ole) = serde_json::from_slice::<OldLeaseEntry>(raw.unwrap().value.as_slice()) {
            let le = LeaseEntry {
                lease_id: ole.lease_id.clone(),
                client_token: ole.client_token.clone(),
                path: ole.path.clone(),
                data: ole
                    .data
                    .clone()
                    .map(|serde_map| serde_map.into_iter().collect())
                    .unwrap_or(Map::new()),
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
    async fn persist_lease_entry(&self, le: &LeaseEntry) -> Result<(), RvError> {
        let value = serde_json::to_string(&le)?;

        let entry = StorageEntry {
            key: le.lease_id.clone(),
            value: value.as_bytes().to_vec(),
        };

        self.id_view.put(&entry).await
    }

    /// Deletes a lease entry from storage.
    async fn delete_lease_entry(&self, lease_id: &str) -> Result<(), RvError> {
        self.id_view.delete(lease_id).await
    }

    /// Creates an index in the token view using the provided token and lease ID.
    async fn create_index_by_token(&self, token: &str, lease_id: &str) -> Result<(), RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
        let key = format!(
            "{}/{}",
            token_store.salt_id(token),
            token_store.salt_id(lease_id)
        );
        let entry = StorageEntry {
            key,
            value: lease_id.as_bytes().to_owned(),
        };
        self.token_view.put(&entry).await
    }

    /// Removes an index from the token view based on the provided token and lease ID.
    async fn remove_index_by_token(&self, token: &str, lease_id: &str) -> Result<(), RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
        let key = format!(
            "{}/{}",
            token_store.salt_id(token),
            token_store.salt_id(lease_id)
        );
        self.token_view.delete(&key).await
    }

    /// Retrieves an index from the token view based on the provided token and lease ID.
    #[allow(dead_code)]
    async fn index_by_token(
        &self,
        token: &str,
        lease_id: &str,
    ) -> Result<Option<StorageEntry>, RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
        let key = format!(
            "{}/{}",
            token_store.salt_id(token),
            token_store.salt_id(lease_id)
        );
        self.token_view.get(&key).await
    }

    /// Looks up lease entries associated with a specific token.
    async fn lookup_by_token(&self, token: &str) -> Result<Vec<String>, RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;
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
    async fn revoke_lease_entry(&self, le: &LeaseEntry) -> Result<(), RvError> {
        let token_store = self
            .token_store
            .read()?
            .upgrade()
            .ok_or(RvError::ErrBarrierSealed)?;

        if le.auth.is_some() {
            return token_store
                .revoke_tree(&le.auth.as_ref().unwrap().client_token)
                .await;
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
