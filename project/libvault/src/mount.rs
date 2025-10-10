//! Simply speaking, the `libvault::mount` module manages the relationship between a 'path' and
//! the real RustyVault module which is responsible for that feature. In RustyVault, everything is
//! exposed to outside by RESTful API, which is defined by 'path'.
//!
//! The binding logic here is managed by `MountEntry` struct.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{select, tick};
use dashmap::DashMap;
use derive_more::Deref;
use lazy_static::lazy_static;
use openssl::{
    hash::MessageDigest,
    pkey::PKey,
    sign::{Signer, Verifier},
};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

use crate::{
    config::MountEntryHMACLevel,
    core::{Core, LogicalBackendNewFunc},
    errors::RvError,
    router::Router,
    storage::{Storage, StorageEntry, barrier::SecurityBarrier, barrier_view::BarrierView},
    utils::{generate_uuid, is_protect_path},
};

pub const LOGICAL_BARRIER_PREFIX: &str = "logical/";
pub const CORE_MOUNT_CONFIG_PATH: &str = "core/mounts";
pub const SYSTEM_BARRIER_PREFIX: &str = "sys/";
pub const MOUNT_TABLE_TYPE: &str = "mounts";

lazy_static! {
    static ref PROTECTED_MOUNTS: Vec<&'static str> = vec!["audit/", "auth/", "sys/",];
    static ref DEFAULT_CORE_MOUNTS: Vec<MountEntry> = vec![
        MountEntry {
            table: MOUNT_TABLE_TYPE.to_string(),
            tainted: false,
            uuid: generate_uuid(),
            path: "secret/".to_string(),
            logical_type: "kv".to_string(),
            description: "key/value secret storage".to_string(),
            ..Default::default()
        },
        MountEntry {
            table: MOUNT_TABLE_TYPE.to_string(),
            tainted: false,
            uuid: generate_uuid(),
            path: "sys/".to_string(),
            logical_type: "system".to_string(),
            description: "system endpoints used for control, policy and debugging".to_string(),
            ..Default::default()
        }
    ];
}

pub struct MountsMonitor {
    core: Arc<Core>,
    interval: u64,
    tables: Arc<RwLock<Vec<Arc<MountsRouter>>>>,
    running: Arc<AtomicBool>,
    stop_condvar: Arc<(Mutex<bool>, Condvar)>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Deref)]
pub struct MountsRouter {
    #[deref]
    pub mounts: Arc<MountTable>,
    pub router: Arc<Router>,
    pub barrier: Arc<dyn SecurityBarrier>,
    pub barrier_prefix: String,
    pub router_prefix: String,
    pub backends: DashMap<String, Arc<LogicalBackendNewFunc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MountEntry {
    #[serde(default)]
    pub table: String,
    pub tainted: bool,
    pub uuid: String,
    pub path: String,
    pub logical_type: String,
    pub description: String,
    pub options: Option<HashMap<String, String>>,
    #[serde(default)]
    pub hmac: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MountTable {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub id: RwLock<String>,
    pub entries: RwLock<HashMap<String, Arc<RwLock<MountEntry>>>>,
}

impl MountsRouter {
    pub fn new(
        mounts: Arc<MountTable>,
        router: Arc<Router>,
        barrier: Arc<dyn SecurityBarrier>,
        barrier_prefix: &str,
        router_prefix: &str,
    ) -> Self {
        Self {
            mounts,
            router,
            barrier,
            barrier_prefix: barrier_prefix.to_string(),
            router_prefix: router_prefix.to_string(),
            backends: DashMap::new(),
        }
    }

    pub fn setup(&self, core: Arc<Core>) -> Result<(), RvError> {
        let mounts = self.mounts.entries.read()?;

        for mount_entry in mounts.values() {
            let entry = mount_entry.read()?;
            let barrier_path = format!("{}{}/", self.barrier_prefix, &entry.uuid);

            let backend_new_func = self.get_backend(&entry.logical_type)?;
            let backend = backend_new_func(core.clone())?;

            let view = BarrierView::new(self.barrier.clone(), &barrier_path);
            let path = format!("{}{}", self.router_prefix, &entry.path);

            self.router
                .mount(backend, &path, mount_entry.clone(), view)?;

            if entry.tainted {
                self.router.taint(&entry.path)?;
            }
        }

        Ok(())
    }

    pub fn unload(&self) -> Result<(), RvError> {
        self.mounts.clear()
    }

    pub fn get_backend(&self, logical_type: &str) -> Result<Arc<LogicalBackendNewFunc>, RvError> {
        if let Some(backend) = self.backends.get(logical_type) {
            Ok(backend.clone())
        } else {
            Err(RvError::ErrCoreLogicalBackendNoExist)
        }
    }

    pub fn add_backend(
        &self,
        logical_type: &str,
        backend: Arc<LogicalBackendNewFunc>,
    ) -> Result<(), RvError> {
        let result = self
            .backends
            .entry(logical_type.to_string())
            .or_try_insert_with(|| Ok::<_, ()>(backend));

        if result.is_err() {
            return Err(RvError::ErrCoreLogicalBackendExist);
        }

        Ok(())
    }

    pub fn delete_backend(&self, logical_type: &str) -> Result<(), RvError> {
        self.backends.remove(logical_type);
        Ok(())
    }
}

impl MountEntry {
    pub fn new(table: &str, path: &str, logical_type: &str, desc: &str) -> Self {
        Self {
            table: table.into(),
            tainted: false,
            uuid: String::new(),
            path: path.to_string(),
            logical_type: logical_type.to_string(),
            description: desc.to_string(),
            options: None,
            hmac: String::new(),
        }
    }

    pub fn calc_hmac(&mut self, key: &[u8]) -> Result<(), RvError> {
        let msg = self.get_hmac_msg();
        let pkey = PKey::hmac(key)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey)?;
        signer.update(msg.as_bytes())?;
        let hmac = signer.sign_to_vec()?;
        self.hmac = hex::encode(hmac.as_slice());

        Ok(())
    }

    pub fn verify_hmac(&self, key: &[u8]) -> Result<bool, RvError> {
        let msg = self.get_hmac_msg();
        let pkey = PKey::hmac(key)?;
        let mut verifier = Verifier::new(MessageDigest::sha256(), &pkey).unwrap();
        verifier.update(msg.as_bytes())?;
        Ok(verifier.verify(self.hmac.as_bytes())?)
    }

    pub fn get_hmac_msg(&self) -> String {
        let mut msg = format!(
            "{}-{}-{}-{}",
            self.table, self.path, self.logical_type, self.description
        );

        if let Some(options) = &self.options {
            let options_btree: BTreeMap<String, String> = options
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (key, value) in options_btree.iter() {
                msg = format!("{msg}-{key}:{value}");
            }
        }

        msg
    }
}

impl MountTable {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            id: RwLock::new(generate_uuid()),
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn clear(&self) -> Result<(), RvError> {
        let mut entries_write = self.entries.write()?;
        entries_write.clear();
        Ok(())
    }

    pub fn get(&self, path: &str) -> Result<Option<Arc<RwLock<MountEntry>>>, RvError> {
        let mounts = self.entries.read()?;
        Ok(mounts.get(path).cloned())
    }

    pub fn delete(&self, path: &str) -> bool {
        match self.entries.write() {
            Ok(mut mounts) => mounts.remove(path).is_some(),
            Err(_) => false,
        }
    }

    pub fn set_taint(&self, path: &str, value: bool) -> bool {
        match self.entries.write() {
            Ok(mounts) => {
                if let Some(mount_entry) = mounts.get(path)
                    && let Ok(mut entry) = mount_entry.write()
                {
                    entry.tainted = value;
                    return true;
                }
            }
            Err(_) => {
                return false;
            }
        }
        false
    }

    pub fn set_default(
        &self,
        mounts: Vec<MountEntry>,
        hmac_key: Option<&[u8]>,
    ) -> Result<(), RvError> {
        let mut table = self.entries.write()?;
        for mut mount in mounts {
            if let Some(key) = hmac_key {
                mount.calc_hmac(key)?;
            }
            table.insert(mount.path.clone(), Arc::new(RwLock::new(mount)));
        }
        Ok(())
    }

    pub async fn load_or_default(
        &self,
        storage: &dyn Storage,
        hmac_key: Option<&[u8]>,
        hmac_level: MountEntryHMACLevel,
    ) -> Result<(), RvError> {
        match self.load(storage, hmac_key, hmac_level).await {
            Err(RvError::ErrConfigLoadFailed) => {
                self.set_default(DEFAULT_CORE_MOUNTS.to_vec(), hmac_key)?;
                self.persist(storage).await?;
                return Ok(());
            }
            Err(err) => {
                return Err(err);
            }
            _ => {}
        }

        self.mount_update(storage, hmac_key, hmac_level).await
    }

    pub async fn load(
        &self,
        storage: &dyn Storage,
        hmac_key: Option<&[u8]>,
        hmac_level: MountEntryHMACLevel,
    ) -> Result<Option<()>, RvError> {
        let entry = storage.get(&self.path).await?;
        if entry.is_none() {
            return Err(RvError::ErrConfigLoadFailed);
        }

        let new_table: MountTable = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        let mut new_entries = new_table.entries.write()?;
        let mut entries = self.entries.write()?;
        let new_id = new_table.id.read()?;
        let mut id = self.id.write()?;

        if id.to_string() == new_id.to_string() && entries.len() == new_entries.len() {
            return Ok(None);
        }

        entries.clear();

        if hmac_level != MountEntryHMACLevel::None && hmac_key.is_some() {
            let key = hmac_key.unwrap();
            new_entries.retain(|_, me| {
                let entry = me.read().unwrap();
                match entry.verify_hmac(key) {
                    Ok(ret) => {
                        if !ret {
                            log::error!(
                                "load mount entry failed, path: {}, err: HMAC validation failed",
                                entry.path
                            );
                        }
                        ret
                    }
                    Err(e) => {
                        log::error!(
                            "load mount entry failed, path: {}, err: {:?}",
                            entry.path,
                            e
                        );
                        false
                    }
                }
            });
        }

        entries.extend(new_entries.drain());
        *id = new_id.to_string();

        Ok(Some(()))
    }

    pub async fn persist(&self, storage: &dyn Storage) -> Result<(), RvError> {
        let value = serde_json::to_string(self)?;
        storage
            .put(&StorageEntry {
                key: self.path.clone(),
                value: value.into_bytes(),
            })
            .await?;
        Ok(())
    }

    async fn mount_update(
        &self,
        storage: &dyn Storage,
        hmac_key: Option<&[u8]>,
        hmac_level: MountEntryHMACLevel,
    ) -> Result<(), RvError> {
        let mut need_persist = false;
        {
            let mounts = self.entries.read()?;

            for mount_entry in mounts.values() {
                let mut entry = mount_entry.write()?;
                if entry.table.is_empty() {
                    entry.table = MOUNT_TABLE_TYPE.to_string();
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
            self.persist(storage).await?;
        }

        Ok(())
    }
}

impl MountsMonitor {
    pub fn new(core: Arc<Core>, interval: u64) -> Self {
        Self {
            core,
            interval,
            tables: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            stop_condvar: Arc::new((Mutex::new(false), Condvar::new())),
            handle: Mutex::new(None),
        }
    }

    pub fn add_mounts_router(&self, table: Arc<MountsRouter>) {
        let mut tables = self.tables.write().unwrap();
        tables.push(table);
    }

    pub fn remove_mounts_router(&self, table: Arc<MountsRouter>) {
        let mut tables = self.tables.write().unwrap();
        tables.retain(|mt| mt.path != table.path);
    }

    pub fn start(&self) {
        if self.running.load(Ordering::Relaxed) {
            return;
        }

        self.running.store(true, Ordering::Relaxed);
        let running_flag = self.running.clone();
        let stop_condvar = self.stop_condvar.clone();

        let core = self.core.clone();
        let mount_tables = self.tables.clone();

        let ticker = tick(Duration::from_secs(self.interval));
        let handle = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                while running_flag.load(Ordering::Relaxed) {
                    select! {
                        recv(ticker) -> _ => {
                            let mut changed = false;
                            let tables = mount_tables.read().unwrap();
                            for table in tables.iter() {
                                match table
                                    .load(
                                        core.barrier.as_storage(),
                                        Some(&core.state.load().hmac_key),
                                        core.mount_entry_hmac_level,
                                    )
                                    .await
                                {
                                    Ok(Some(())) => changed = true,
                                    _ => continue,
                                }
                            }

                            if changed {
                                let _ = core.router.clear();

                                for table in tables.iter() {
                                    if let Err(err) = table.setup(core.clone()) {
                                        log::error!("update mount table failed, path: {}, err: {:?}", table.path, err);
                                    }
                                }
                            }
                        }
                        default => {
                            let (stop_mutex, stop_condvar) = &*stop_condvar;
                            let stop_guard = stop_mutex.lock().unwrap();
                            if *stop_guard {
                                break;
                            }
                            let _ = stop_condvar.wait_timeout(stop_guard, Duration::from_millis(10)).unwrap();
                        }
                    }
                }
            });
        });

        self.handle.lock().unwrap().replace(handle);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let (stop_mutex, stop_condvar) = &*self.stop_condvar;
        {
            let mut stop_guard = stop_mutex.lock().unwrap();
            *stop_guard = true;
        }
        stop_condvar.notify_one();
        if let Some(handle) = self.handle.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

impl Core {
    pub async fn mount(&self, me: &MountEntry) -> Result<(), RvError> {
        {
            let mut table = self.mounts_router.entries.write()?;
            let mut entry = me.clone();

            if !entry.path.ends_with('/') {
                entry.path += "/";
            }

            if is_protect_path(&PROTECTED_MOUNTS, &[&entry.path]) {
                return Err(RvError::ErrMountPathProtected);
            }

            if entry.table.is_empty() {
                entry.table = MOUNT_TABLE_TYPE.to_string();
            }

            let match_mount_path = self.router.matching_mount(&entry.path)?;
            if !match_mount_path.is_empty() {
                return Err(RvError::ErrMountPathExist);
            }

            let backend_new_func = self.get_logical_backend(&me.logical_type)?;
            let backend = backend_new_func(self.self_ptr.upgrade().unwrap().clone())?;

            entry.uuid = generate_uuid();

            let prefix = format!("{}{}/", LOGICAL_BARRIER_PREFIX, &entry.uuid);
            let view = BarrierView::new(self.barrier.clone(), &prefix);

            let path = entry.path.clone();

            entry.calc_hmac(&self.state.load().hmac_key)?;

            let mount_entry = Arc::new(RwLock::new(entry));

            self.router
                .mount(backend, &path, mount_entry.clone(), view)?;

            table.insert(path, mount_entry);
        }

        self.mounts_router
            .persist(self.barrier.as_storage())
            .await?;

        Ok(())
    }

    pub async fn unmount(&self, path: &str) -> Result<(), RvError> {
        let mut path = path.to_string();
        if !path.ends_with('/') {
            path += "/";
        }

        if is_protect_path(&PROTECTED_MOUNTS, &[&path]) {
            return Err(RvError::ErrMountPathProtected);
        }

        let match_mount = self.router.matching_mount(&path)?;
        if match_mount.is_empty() || match_mount != path {
            return Err(RvError::ErrMountNotMatch);
        }

        let view = self.router.matching_view(&path)?;

        self.taint_mount_entry(&path).await?;

        self.router.taint(&path)?;

        self.router.unmount(&path)?;

        if view.is_some() {
            view.unwrap().clear().await?;
        }

        self.remove_mount_entry(&path).await?;

        Ok(())
    }

    pub async fn remount(&self, src: &str, dst: &str) -> Result<(), RvError> {
        let mut src = src.to_string();
        let mut dst = dst.to_string();

        if !src.ends_with('/') {
            src += "/";
        }

        if !dst.ends_with('/') {
            dst += "/";
        }

        if is_protect_path(&PROTECTED_MOUNTS, &[&src, &dst]) {
            return Err(RvError::ErrMountPathProtected);
        }

        let dst_match = self.router.matching_mount(&dst)?;
        if !dst_match.is_empty() {
            return Err(RvError::ErrMountPathExist);
        }

        let Some(src_match) = self.router.matching_mount_entry(&src)? else {
            return Err(RvError::ErrMountNotMatch);
        };

        let src_path;
        {
            let mut src_entry = src_match.write()?;
            src_entry.tainted = true;

            self.router.taint(&src)?;

            if !(self.router.matching_mount(&dst)?).is_empty() {
                return Err(RvError::ErrMountPathExist);
            }

            src_path = src_entry.path.clone();
            src_entry.path.clone_from(&dst);
            src_entry.tainted = false;
            src_entry.calc_hmac(&self.state.load().hmac_key)?;

            std::mem::drop(src_entry);
        }

        if let Err(e) = self.mounts_router.persist(self.barrier.as_storage()).await {
            let mut src_entry = src_match.write()?;
            src_entry.path = src_path;
            src_entry.tainted = true;
            src_entry.calc_hmac(&self.state.load().hmac_key)?;
            return Err(e);
        }

        self.router.remount(&dst, &src)?;

        self.router.untaint(&dst)?;

        Ok(())
    }

    pub fn unload_mounts(&self) -> Result<(), RvError> {
        let _ = self.router.clear();
        let _ = self.mounts_router.clear();
        Ok(())
    }

    async fn taint_mount_entry(&self, path: &str) -> Result<(), RvError> {
        if self.mounts_router.set_taint(path, true) {
            self.mounts_router
                .persist(self.barrier.as_storage())
                .await?;
        }
        Ok(())
    }

    async fn remove_mount_entry(&self, path: &str) -> Result<(), RvError> {
        if self.mounts_router.delete(path) {
            self.mounts_router
                .persist(self.barrier.as_storage())
                .await?;
        }
        Ok(())
    }
}
