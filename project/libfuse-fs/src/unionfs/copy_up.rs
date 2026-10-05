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

/// Confirmed ownership of one exact copy-up source handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopyUpHandleState {
    /// The exact handle still owns an open resource and can be released.
    Open,
    /// The backend confirms that this exact handle is already closed.
    Closed,
    /// Closure could not be confirmed; do not retry or discard ownership.
    Unknown,
}

/// A copy-up failure together with its independently observed cleanup state.
#[derive(Clone, Debug)]
pub struct CopyUpCleanupFailure {
    /// Primary operation errno, retained even when cleanup also failed.
    pub primary_errno: Option<i32>,
    /// The actual source RELEASE errno, when available.
    pub cleanup_errno: Option<i32>,
    /// Ownership observation after the failed RELEASE.
    pub ownership: CopyUpHandleState,
    primary: String,
    cleanup: String,
}

impl std::fmt::Display for CopyUpCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; source cleanup failed: {} (ownership: {:?})",
            self.primary, self.cleanup, self.ownership
        )
    }
}

impl std::error::Error for CopyUpCleanupFailure {}

pub(super) fn kernel_error(error: Error) -> Error {
    let Some(failure) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<CopyUpCleanupFailure>())
    else {
        return error;
    };
    // asyncfuse maps custom io::Error payloads without a raw errno to EIO.
    // The kernel receives the primary errno; the typed secondary cleanup
    // failure remains in the task and the bounded diagnostic reports.
    Error::from_raw_os_error(
        failure
            .primary_errno
            .or(failure.cleanup_errno)
            .unwrap_or(libc::EIO),
    )
}

#[derive(Clone)]
struct SourceHandle {
    layer: Arc<BoxedLayer>,
    ctx: Request,
    inode: u64,
    handle: u64,
}

type SourceOwner = Arc<std::sync::Mutex<Option<SourceHandle>>>;

fn cleanup_failure(primary: Option<&Error>, cleanup: Error, ownership: CopyUpHandleState) -> Error {
    let kind = primary.map_or(cleanup.kind(), Error::kind);
    let previous = primary.and_then(|error| {
        error
            .get_ref()
            .and_then(|error| error.downcast_ref::<CopyUpCleanupFailure>())
    });
    let primary_errno = previous.map_or_else(
        || primary.map_or(cleanup.raw_os_error(), Error::raw_os_error),
        |failure| failure.primary_errno,
    );
    let primary = previous.map_or_else(
        || primary.map_or_else(|| cleanup.to_string(), ToString::to_string),
        |failure| failure.primary.clone(),
    );
    Error::new(
        kind,
        CopyUpCleanupFailure {
            primary_errno,
            cleanup_errno: cleanup.raw_os_error(),
            ownership,
            primary,
            cleanup: cleanup.to_string(),
        },
    )
}

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

struct Failure {
    raw: Option<i32>,
    kind: std::io::ErrorKind,
    message: String,
    cleanup: Option<CopyUpCleanupFailure>,
}

impl Failure {
    fn new(error: &Error) -> Self {
        Self {
            raw: error.raw_os_error(),
            kind: error.kind(),
            message: error.to_string(),
            cleanup: error
                .get_ref()
                .and_then(|e| e.downcast_ref::<CopyUpCleanupFailure>())
                .cloned(),
        }
    }

    fn error(&self) -> Error {
        if let Some(cleanup) = &self.cleanup {
            return Error::new(self.kind, cleanup.clone());
        }
        match self.raw {
            Some(raw) => Error::from_raw_os_error(raw),
            None => Error::new(self.kind, self.message.clone()),
        }
    }
}

struct Outcome {
    node: Option<Arc<OverlayInode>>,
    error: Option<Failure>,
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
                error: Some(Failure::new(error)),
            },
        }
    }

    fn result(&self) -> Result<Arc<OverlayInode>> {
        if let Some(node) = &self.node {
            return Ok(node.clone());
        }
        Err(self.error.as_ref().expect("copy-up outcome").error())
    }
}

struct TaskState {
    future: Option<CopyFuture>,
    outcome: Option<Outcome>,
    recovery: Option<Pin<Box<dyn Future<Output = Result<()>> + Send>>>,
    recovered: Option<std::result::Result<(), Failure>>,
}

pub(super) struct CopyUpTask {
    cancellation: Arc<Cancellation>,
    complete: AtomicBool,
    finished: AtomicBool,
    source: SourceOwner,
    state: Mutex<TaskState>,
}

impl CopyUpTask {
    pub(super) fn new(ctx: Request, node: Arc<OverlayInode>) -> Self {
        let cancellation = Arc::new(Cancellation::new());
        let source = Arc::new(std::sync::Mutex::new(None));
        let future = Box::pin(copy_file(ctx, node, cancellation.clone(), source.clone()));
        Self {
            cancellation,
            complete: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            source,
            state: Mutex::new(TaskState {
                future: Some(future),
                outcome: None,
                recovery: None,
                recovered: None,
            }),
        }
    }

    pub(super) fn cancelled_or_complete(&self) -> bool {
        self.cancellation.cancelled.load(Ordering::Acquire) || self.finished.load(Ordering::Acquire)
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
        self.complete.store(
            self.source.lock().expect("source ownership").is_none(),
            Ordering::Release,
        );
        self.finished.store(true, Ordering::Release);
        result
    }

    pub(super) async fn recover(&self) -> Result<()> {
        let primary = self.wait().await;
        let mut state = self.state.lock().await;
        // Another recovery can finish after wait() releases this lock. Its
        // confirmed result supersedes the original, possibly Open, failure.
        if self.complete() {
            if let Some(result) = &state.recovered {
                return result.as_ref().map(|_| ()).map_err(Failure::error);
            }
            return match primary {
                Err(error)
                    if error
                        .get_ref()
                        .is_some_and(|e| e.is::<CopyUpCleanupFailure>()) =>
                {
                    Err(error)
                }
                _ => Ok(()),
            };
        }
        if state.recovery.is_none() {
            state.recovery = Some(Box::pin(recover_source(self.source.clone(), primary.err())));
        }
        // A cancelled recovery caller also retains its in-flight RELEASE and
        // observation future, preventing a guessed retry or lost ownership.
        let result = state.recovery.as_mut().expect("source recovery").await;
        state.recovery.take();
        state.recovered = Some(result.as_ref().map(|_| ()).map_err(Failure::new));
        self.complete.store(
            self.source.lock().expect("source ownership").is_none(),
            Ordering::Release,
        );
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
    owner: &SourceOwner,
) -> std::result::Result<(), (Error, CopyUpHandleState)> {
    let source = owner
        .lock()
        .expect("source ownership")
        .clone()
        .expect("open source handle");
    match source
        .layer
        .release(
            source.ctx,
            source.inode,
            source.handle,
            libc::O_RDONLY as u32,
            0,
            false,
        )
        .await
    {
        Ok(()) => {
            owner.lock().expect("source ownership").take();
            Ok(())
        }
        Err(error) => {
            let error: Error = error.into();
            let ownership = source
                .layer
                .copy_up_handle_state(source.inode, source.handle)
                .await
                .unwrap_or(CopyUpHandleState::Unknown);
            if ownership == CopyUpHandleState::Closed {
                owner.lock().expect("source ownership").take();
            }
            Err((error, ownership))
        }
    }
}

async fn recover_source(owner: SourceOwner, primary: Option<Error>) -> Result<()> {
    let source = owner.lock().expect("source ownership").clone();
    let Some(source) = source else {
        return Ok(());
    };
    let ownership = source
        .layer
        .copy_up_handle_state(source.inode, source.handle)
        .await
        .map_err(|error| cleanup_failure(primary.as_ref(), error, CopyUpHandleState::Unknown))?;
    match ownership {
        CopyUpHandleState::Closed => {
            owner.lock().expect("source ownership").take();
            Ok(())
        }
        CopyUpHandleState::Open => release_source(&owner)
            .await
            .map_err(|(error, ownership)| cleanup_failure(primary.as_ref(), error, ownership)),
        CopyUpHandleState::Unknown => Err(cleanup_failure(
            primary.as_ref(),
            Error::other("source closure remains unknown; no RELEASE was retried"),
            ownership,
        )),
    }
}

async fn copy_file(
    ctx: Request,
    node: Arc<OverlayInode>,
    cancellation: Arc<Cancellation>,
    source: SourceOwner,
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
    *source.lock().expect("source ownership") = Some(SourceHandle {
        layer: lower.clone(),
        ctx,
        inode: lower_inode,
        handle,
    });
    let mut source_release_attempted = false;
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
        source_release_attempted = true;
        release_source(&source).await.map_err(|(error, ownership)| cleanup_failure(None, error, ownership))?;
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
    if !source_release_attempted {
        if let Err((error, ownership)) = release_source(&source).await {
            return Err(cleanup_failure(result.as_ref().err(), error, ownership));
        }
    }
    result
}
