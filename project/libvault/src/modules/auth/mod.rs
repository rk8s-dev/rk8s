//! This module provides the 'auth/' path and relevant authentication features.
//!
//! After a successful authentication, a client will be granted a token for further operations.

use std::{
    any::Any,
    sync::{Arc, RwLock},
};

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use lazy_static::lazy_static;

use crate::{
    config::MountEntryHMACLevel,
    core::{Core, LogicalBackendNewFunc},
    errors::RvError,
    handler::{AuthHandler, Handler},
    logical::Backend,
    modules::Module,
    mount::{MountEntry, MountTable, MountsRouter},
    rv_error_response_status,
    storage::{barrier::SecurityBarrier, barrier_view::BarrierView},
    utils::{generate_uuid, is_protect_path},
};

pub mod expiration;
pub mod token_store;
pub use expiration::ExpirationManager;
pub use token_store::TokenStore;

const AUTH_CONFIG_PATH: &str = "core/auth";
const AUTH_BARRIER_PREFIX: &str = "auth/";

pub const AUTH_ROUTER_PREFIX: &str = "auth/";
pub const AUTH_TABLE_TYPE: &str = "auth";

lazy_static! {
    static ref PROTECTED_AUTHS: Vec<&'static str> = vec!["auth/token",];
    static ref DEFAULT_AUTH_MOUNTS: Vec<MountEntry> = vec![MountEntry {
        table: AUTH_TABLE_TYPE.to_string(),
        tainted: false,
        uuid: generate_uuid(),
        path: "token/".to_string(),
        logical_type: "token".to_string(),
        description: "token based credentials".to_string(),
        ..Default::default()
    }];
}

pub struct AuthModule {
    pub name: String,
    pub core: Arc<Core>,
    pub barrier: Arc<dyn SecurityBarrier>,
    pub mounts_router: Arc<MountsRouter>,
    pub token_store: ArcSwapOption<TokenStore>,
    pub expiration: ArcSwapOption<ExpirationManager>,
}

impl AuthModule {
    pub fn new(core: Arc<Core>) -> Result<Self, RvError> {
        Ok(Self {
            name: "auth".to_string(),
            core: core.clone(),
            barrier: core.barrier.clone(),
            mounts_router: Arc::new(MountsRouter::new(
                Arc::new(MountTable::new(AUTH_CONFIG_PATH)),
                core.router.clone(),
                core.barrier.clone(),
                AUTH_BARRIER_PREFIX,
                AUTH_ROUTER_PREFIX,
            )),
            token_store: ArcSwapOption::empty(),
            expiration: ArcSwapOption::empty(),
        })
    }

    pub fn set_auth_handlers(&self, handlers: Arc<Vec<Arc<dyn AuthHandler>>>) {
        self.token_store
            .load()
            .as_ref()
            .unwrap()
            .auth_handlers
            .store(handlers);
    }

    pub async fn enable_auth(&self, me: &MountEntry) -> Result<(), RvError> {
        let mounts_router = &self.mounts_router;
        {
            let mut auth_table = mounts_router.mounts.entries.write()?;
            let mut entry = me.clone();

            if !entry.path.ends_with('/') {
                entry.path += "/";
            }

            if entry.path == "/" {
                return Err(RvError::ErrMountPathProtected);
            }

            if entry.logical_type == "token" {
                return Err(RvError::ErrMountFailed);
            }

            if is_protect_path(&PROTECTED_AUTHS, &[&entry.path]) {
                return Err(RvError::ErrMountPathProtected);
            }

            for (_, mount_entry) in auth_table.iter() {
                let ent = mount_entry.read()?;
                if ent.path.starts_with(&entry.path) || entry.path.starts_with(&ent.path) {
                    return Err(rv_error_response_status!(
                        409,
                        &format!("path is already in use at {}", &entry.path)
                    ));
                }
            }

            let match_mount_path = mounts_router.router.matching_mount(&entry.path)?;
            if !match_mount_path.is_empty() {
                return Err(rv_error_response_status!(
                    409,
                    &format!("path is already in use at {match_mount_path}")
                ));
            }

            let backend_new_func = self.get_auth_backend(&entry.logical_type)?;
            let backend = backend_new_func(self.core.clone())?;

            entry.uuid = generate_uuid();

            let prefix = format!("{}{}/", AUTH_BARRIER_PREFIX, &entry.uuid);
            let view = BarrierView::new(self.barrier.clone(), &prefix);

            let path = format!("{}{}", AUTH_ROUTER_PREFIX, &entry.path);
            let key = entry.path.clone();

            let mount_entry = Arc::new(RwLock::new(entry));

            mounts_router
                .router
                .mount(backend, &path, mount_entry.clone(), view)?;

            auth_table.insert(key, mount_entry);
        }

        mounts_router.persist(self.barrier.as_storage()).await?;

        Ok(())
    }

    pub async fn disable_auth(&self, path: &str) -> Result<(), RvError> {
        let mounts_router = &self.mounts_router;

        let mut path = path.to_string();
        if !path.ends_with('/') {
            path += "/";
        }

        if path == "token/" {
            return Err(RvError::ErrMountPathProtected);
        }

        let full_path = format!("{}{}", AUTH_ROUTER_PREFIX, &path);
        let view = mounts_router.router.matching_view(&full_path)?;

        self.taint_auth_entry(&path).await?;

        mounts_router.router.taint(&full_path)?;

        mounts_router.router.unmount(&full_path)?;

        if view.is_some() {
            view.unwrap().clear().await?;
        }

        self.remove_auth_entry(&path).await?;

        Ok(())
    }

    pub async fn remount_auth(&self, src: &str, dst: &str) -> Result<(), RvError> {
        let mut src = src.to_string();
        let mut dst = dst.to_string();

        if !src.ends_with('/') {
            src += "/";
        }

        if !dst.ends_with('/') {
            dst += "/";
        }

        if !src.starts_with(AUTH_ROUTER_PREFIX) {
            return Err(rv_error_response_status!(
                400,
                &format!("cannot remount non-auth mount {src}")
            ));
        }

        if !dst.starts_with(AUTH_ROUTER_PREFIX) {
            return Err(rv_error_response_status!(
                400,
                &format!("cannot remount auth mount to non-auth mount {dst}")
            ));
        }

        if is_protect_path(&PROTECTED_AUTHS, &[&src, &dst]) {
            return Err(RvError::ErrMountPathProtected);
        }

        let mounts_router = &self.mounts_router;

        let dst_match = mounts_router.router.matching_mount(&dst)?;
        if !dst_match.is_empty() {
            return Err(RvError::ErrMountPathExist);
        }

        let Some(src_match) = mounts_router.router.matching_mount_entry(&src)? else {
            return Err(RvError::ErrMountNotMatch);
        };

        let src_path;
        {
            let mut src_entry = src_match.write()?;
            src_entry.tainted = true;

            mounts_router.router.taint(&src)?;

            if !(mounts_router.router.matching_mount(&dst)?).is_empty() {
                return Err(RvError::ErrMountPathExist);
            }

            src_path = src_entry.path.clone();
            src_entry.path = dst
                .as_str()
                .trim_start_matches(AUTH_ROUTER_PREFIX)
                .to_string();
            src_entry.tainted = false;

            std::mem::drop(src_entry);
        }

        if let Err(e) = mounts_router
            .mounts
            .persist(self.barrier.as_storage())
            .await
        {
            let mut src_entry = src_match.write()?;
            src_entry.path = src_path;
            src_entry.tainted = true;
            return Err(e);
        }

        mounts_router.router.remount(&dst, &src)?;

        mounts_router.router.untaint(&dst)?;

        Ok(())
    }

    pub async fn remove_auth_entry(&self, path: &str) -> Result<(), RvError> {
        if self.mounts_router.delete(path) {
            self.mounts_router
                .persist(self.barrier.as_storage())
                .await?;
        }
        Ok(())
    }

    pub async fn taint_auth_entry(&self, path: &str) -> Result<(), RvError> {
        if self.mounts_router.set_taint(path, true) {
            self.mounts_router
                .persist(self.barrier.as_storage())
                .await?;
        }
        Ok(())
    }

    pub fn teardown_auth(&self) -> Result<(), RvError> {
        let _ = self.mounts_router.mounts.clear();
        // TODO
        let _ = self.mounts_router.router.clear();
        Ok(())
    }

    pub async fn load_auth(
        &self,
        hmac_key: Option<&[u8]>,
        hmac_level: MountEntryHMACLevel,
    ) -> Result<(), RvError> {
        let mounts_router = &self.mounts_router;
        if mounts_router
            .mounts
            .load(self.barrier.as_storage(), hmac_key, hmac_level)
            .await
            .is_err()
        {
            mounts_router
                .mounts
                .set_default(DEFAULT_AUTH_MOUNTS.to_vec(), hmac_key)?;
            mounts_router
                .mounts
                .persist(self.barrier.as_storage())
                .await?;
        }

        self.update_auth_mount(hmac_key, hmac_level).await
    }

    pub async fn persist_auth(&self) -> Result<(), RvError> {
        self.mounts_router.persist(self.barrier.as_storage()).await
    }

    pub fn setup_auth(&self) -> Result<(), RvError> {
        self.mounts_router.setup(self.core.clone())
    }

    pub fn get_auth_backend(
        &self,
        logical_type: &str,
    ) -> Result<Arc<LogicalBackendNewFunc>, RvError> {
        self.mounts_router.get_backend(logical_type)
    }

    pub fn add_auth_backend(
        &self,
        logical_type: &str,
        backend: Arc<LogicalBackendNewFunc>,
    ) -> Result<(), RvError> {
        self.mounts_router.add_backend(logical_type, backend)
    }

    pub fn delete_auth_backend(&self, logical_type: &str) -> Result<(), RvError> {
        self.mounts_router.delete_backend(logical_type)
    }

    async fn update_auth_mount(
        &self,
        hmac_key: Option<&[u8]>,
        hmac_level: MountEntryHMACLevel,
    ) -> Result<(), RvError> {
        let mut need_persist = false;
        {
            let mounts = self.mounts_router.mounts.entries.read()?;

            for mount_entry in mounts.values() {
                let mut entry = mount_entry.write()?;
                if entry.table.is_empty() {
                    entry.table = AUTH_TABLE_TYPE.to_string();
                    need_persist = true;
                }

                if entry.hmac.is_empty()
                    && hmac_key.is_some()
                    && hmac_level == MountEntryHMACLevel::Compat
                {
                    entry.calc_hmac(hmac_key.unwrap())?;
                    need_persist = true;
                }
            }
        }

        if need_persist {
            self.persist_auth().await?;
        }

        Ok(())
    }
}

#[async_trait]
impl Module for AuthModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    async fn init(&self, core: &Core) -> Result<(), RvError> {
        let expiration = ExpirationManager::new(core)?.wrap();
        let token_store = TokenStore::new(core, expiration.clone()).await?.wrap();

        expiration.set_token_store(&token_store)?;

        self.expiration.store(Some(expiration.clone()));
        self.token_store.store(Some(token_store.clone()));

        let ts = token_store.clone();

        let token_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut backend = token_store.new_backend();
            backend.init()?;
            Ok(Arc::new(backend))
        };

        self.add_auth_backend("token", Arc::new(token_backend_new_func))?;
        self.load_auth(
            Some(&core.state.load().hmac_key),
            core.mount_entry_hmac_level,
        )
        .await?;
        self.setup_auth()?;

        if let Some(mounts_monitor) = core.mounts_monitor.load().as_ref() {
            mounts_monitor.add_mounts_router(self.mounts_router.clone());
        }

        expiration.restore().await?;
        expiration.start_check_expired_lease_entries();

        core.add_handler(ts as Arc<dyn Handler>)?;

        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        if let Some(mounts_monitor) = core.mounts_monitor.load().as_ref() {
            mounts_monitor.remove_mounts_router(self.mounts_router.clone());
        }
        core.delete_handler(self.token_store.load().as_ref().unwrap().clone() as Arc<dyn Handler>)?;
        self.delete_auth_backend("token")?;
        self.teardown_auth()?;
        Ok(())
    }
}
