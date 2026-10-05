//! Private, bounded regular-file copy-up and explicit cancellation recovery.

use super::{BoxedLayer, OverlayInode, RealInode};
use crate::context::OperationContext;
use crate::util::convert_stat64_to_file_attr;
use async_trait::async_trait;
use asyncfuse::raw::{Request, reply::ReplyAttr};
use asyncfuse::{FileType, mode_from_kind_and_perm};
use std::ffi::OsStr;
use std::future::Future;
use std::io::{Error, Result};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, Notify};

/// A private destination owned by one copy-up operation.
///
/// Drop must synchronously close its actual destination resources and remove
/// unpublished storage. `promote` publishes only complete bytes, without
/// replacing an existing name; until `commit`, Drop must undo its own promotion.
/// Storage must be outside the upper namespace, including direct upper scans.
#[async_trait]
pub trait CopyUpFile: Send {
    /// Write at most the provided bytes at the given absolute offset.
    async fn write(&mut self, offset: u64, data: &[u8]) -> Result<u32>;

    /// Atomically expose the complete file under one destination name.
    fn promote(&mut self, name: &OsStr) -> Result<()>;

    /// Verify the promoted name still names this exact destination inode.
    fn verify_promotion(&self) -> Result<()>;

    /// Retain the promoted name after the upper inode has been installed.
    fn commit(&mut self);
}

pub(super) const MAX_PENDING_COPYUPS: usize = 64;
const COPY_BLOCK: u32 = 4 * 1024 * 1024;
type CopyFuture = Pin<Box<dyn Future<Output = Result<Arc<OverlayInode>>> + Send>>;

pub(super) struct Cancellation {
    cancelled: AtomicBool,
    wake: Notify,
}

impl Cancellation {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            wake: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.wake.notify_one();
    }

    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(Error::from_raw_os_error(libc::ECANCELED))
        } else {
            Ok(())
        }
    }

    async fn notified(&self) {
        loop {
            let wake = self.wake.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            if self.check().is_err() {
                return;
            }
            wake.await;
        }
    }
}

struct Outcome {
    node: Option<Arc<OverlayInode>>,
    error: Option<(Option<i32>, std::io::ErrorKind, String)>,
}

impl Outcome {
    fn new(result: &Result<Arc<OverlayInode>>) -> Self {
        match result {
            Ok(node) => Self {
                node: Some(node.clone()),
                error: None,
            },
            Err(error) => Self {
                node: None,
                error: Some((error.raw_os_error(), error.kind(), error.to_string())),
            },
        }
    }

    fn result(&self) -> Result<Arc<OverlayInode>> {
        if let Some(node) = &self.node {
            return Ok(node.clone());
        }
        let (raw, kind, message) = self.error.as_ref().expect("copy-up outcome");
        Err(match raw {
            Some(raw) => Error::from_raw_os_error(*raw),
            None => Error::new(*kind, message.clone()),
        })
    }
}

struct TaskState {
    future: Option<CopyFuture>,
    outcome: Option<Outcome>,
}

pub(super) struct CopyUpTask {
    cancellation: Arc<Cancellation>,
    complete: AtomicBool,
    state: Mutex<TaskState>,
}

impl CopyUpTask {
    pub(super) fn new(ctx: Request, node: Arc<OverlayInode>) -> Self {
        let cancellation = Arc::new(Cancellation::new());
        let future = Box::pin(copy_file(ctx, node, cancellation.clone()));
        Self {
            cancellation,
            complete: AtomicBool::new(false),
            state: Mutex::new(TaskState {
                future: Some(future),
                outcome: None,
            }),
        }
    }

    pub(super) fn cancelled_or_complete(&self) -> bool {
        self.cancellation.cancelled.load(Ordering::Acquire) || self.complete.load(Ordering::Acquire)
    }

    pub(super) fn complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    pub(super) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(super) async fn wait(&self) -> Result<Arc<OverlayInode>> {
        let mut state = self.state.lock().await;
        if let Some(outcome) = &state.outcome {
            return outcome.result();
        }
        // A cancelled waiter never drops this future: its real open/release
        // sequence remains in the bounded ledger until recovery drives it.
        let result = state.future.as_mut().expect("pending copy-up").await;
        state.future.take();
        state.outcome = Some(Outcome::new(&result));
        self.complete.store(true, Ordering::Release);
        result
    }

    pub(super) async fn wait_for_caller(&self) -> Result<Arc<OverlayInode>> {
        let mut guard = WaiterGuard {
            cancellation: self.cancellation.clone(),
            complete: false,
        };
        let result = self.wait().await;
        guard.complete = true;
        result
    }
}

struct WaiterGuard {
    cancellation: Arc<Cancellation>,
    complete: bool,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        if !self.complete {
            self.cancellation.cancel();
        }
    }
}

async fn release_source(
    layer: &Arc<BoxedLayer>,
    ctx: Request,
    inode: u64,
    handle: u64,
) -> Result<()> {
    match layer
        .release(ctx, inode, handle, libc::O_RDONLY as u32, 0, false)
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let error: Error = error.into();
            if error.raw_os_error() == Some(libc::ENOSYS) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

async fn copy_file(
    ctx: Request,
    node: Arc<OverlayInode>,
    cancellation: Arc<Cancellation>,
) -> Result<Arc<OverlayInode>> {
    cancellation.check()?;
    if node.in_upper_layer().await {
        return Ok(node);
    }
    let parent = node
        .parent
        .lock()
        .await
        .upgrade()
        .ok_or_else(|| Error::other("copy-up has no parent"))?;
    let (lower, _, lower_inode) = node.first_layer_inode().await;
    let (stat, _) = lower.getattr_with_mapping(lower_inode, None, false).await?;
    let attr = convert_stat64_to_file_attr(stat);
    if attr.kind != FileType::RegularFile {
        return Err(Error::from_raw_os_error(libc::EINVAL));
    }
    if !parent.in_upper_layer().await {
        parent.clone().create_upper_dir(ctx, None).await?;
    }
    let (upper, in_upper, parent_inode) = parent.first_layer_inode().await;
    if !in_upper {
        return Err(Error::from_raw_os_error(libc::EROFS));
    }
    cancellation.check()?;

    // OPEN is allowed to finish even if the caller cancels while it is pending.
    // The retained future then receives the real handle and releases it once.
    let handle = lower
        .open(ctx, lower_inode, libc::O_RDONLY as u32)
        .await?
        .fh;
    let mut source_released = false;
    let result = async {
        cancellation.check()?;
        let op_ctx = OperationContext::with_credentials(ctx, attr.uid, attr.gid);
        let mut destination = upper.begin_copy_up(op_ctx, parent_inode, mode_from_kind_and_perm(attr.kind, attr.perm)).await?;
        let mut offset = 0u64;
        while offset < attr.size {
            cancellation.check()?;
            let wanted = (attr.size - offset).min(u64::from(COPY_BLOCK)) as u32;
            let data = tokio::select! {
                biased;
                _ = cancellation.notified() => return Err(Error::from_raw_os_error(libc::ECANCELED)),
                data = lower.read(ctx, lower_inode, handle, offset, wanted) => data?,
            };
            if data.data.is_empty() || data.data.len() > wanted as usize { return Err(Error::from_raw_os_error(libc::EIO)); }
            let mut written = 0usize;
            while written < data.data.len() {
                cancellation.check()?;
                let position = offset.checked_add(written as u64).ok_or_else(|| Error::from_raw_os_error(libc::EOVERFLOW))?;
                let n = tokio::select! {
                    biased;
                    _ = cancellation.notified() => return Err(Error::from_raw_os_error(libc::ECANCELED)),
                    n = destination.write(position, &data.data[written..]) => n?,
                } as usize;
                if n == 0 || n > data.data.len() - written { return Err(Error::from_raw_os_error(libc::EIO)); }
                written += n;
            }
            offset = offset.checked_add(written as u64).ok_or_else(|| Error::from_raw_os_error(libc::EOVERFLOW))?;
        }
        let eof = tokio::select! {
            biased;
            _ = cancellation.notified() => return Err(Error::from_raw_os_error(libc::ECANCELED)),
            data = lower.read(ctx, lower_inode, handle, offset, 1) => data?,
        };
        if !eof.data.is_empty() { return Err(Error::from_raw_os_error(libc::EIO)); }
        source_released = true;
        release_source(&lower, ctx, lower_inode, handle).await?;
        cancellation.check()?;

        // Namespace promotion and ownership commit use the fixed node lock.
        // Any error/cancellation before commit drops the private destination
        // and rolls back only its own promoted inode.
        let mut real_inodes = node.real_inodes.lock().await;
        cancellation.check()?;
        if real_inodes.first().is_some_and(|inode| inode.in_upper_layer) { return Ok(node.clone()); }
        let name = node.name.read().await;
        cancellation.check()?;
        destination.promote(OsStr::new(name.as_str()))?;
        let entry = upper.lookup(ctx, parent_inode, OsStr::new(name.as_str())).await?;
        let real = RealInode { layer: upper.clone(), in_upper_layer: true, inode: entry.attr.ino, whiteout: false, opaque: false, stat: Some(ReplyAttr { ttl: entry.ttl, attr: entry.attr }) };
        cancellation.check()?;
        destination.verify_promotion()?;
        real_inodes.insert(0, Arc::new(real));
        destination.commit();
        Ok(node.clone())
    }.await;
    if !source_released {
        if let Err(error) = release_source(&lower, ctx, lower_inode, handle).await {
            if result.is_ok()
                || result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.raw_os_error() == Some(libc::ECANCELED))
            {
                return Err(error);
            }
            tracing::error!("copy-up source cleanup failed after its primary error: {error}");
        }
    }
    result
}
