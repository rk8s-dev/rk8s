#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::num::NonZeroU32;
#[allow(unused_imports)]
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
#[allow(unused_imports)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::pin::{pin, Pin};
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

#[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
use async_fs::read_dir;
#[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
use async_global_executor::{self as task, Task as JoinHandle};
#[cfg(all(
    target_os = "linux",
    not(feature = "tokio-runtime"),
    feature = "async-io-runtime",
    feature = "unprivileged"
))]
use async_process::Command;
use bincode::Options;
use bytes::Bytes;
use futures_channel::mpsc::{
    channel, unbounded, Receiver, Sender, UnboundedReceiver, UnboundedSender,
};
use futures_util::future::{Either, FutureExt};
use futures_util::select;
use futures_util::sink::{Sink, SinkExt};
use futures_util::stream::StreamExt;
use nix::mount;
#[cfg(any(target_os = "freebsd", target_os = "macos"))]
use nix::mount::MntFlags;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(all(
    target_os = "linux",
    not(feature = "async-io-runtime"),
    feature = "tokio-runtime",
    feature = "unprivileged"
))]
use tokio::process::Command;
#[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
use tokio::task::JoinHandle;
#[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
use tokio::{fs::read_dir, task};
use tracing::{debug, debug_span, error, instrument, warn, Instrument, Span};

#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use crate::find_fusermount3;
use crate::helper::*;
use crate::notify::Notify;
use crate::raw::abi::*;
#[cfg(any(feature = "async-io-runtime", feature = "tokio-runtime"))]
use crate::raw::connection::FuseConnection;
use crate::raw::filesystem::Filesystem;
use crate::raw::reply::ReplyXAttr;
use crate::raw::request::Request;
use crate::raw::FuseData;
use crate::MountOptions;
use crate::{Errno, SetAttr};

/// A Future which returns when a file system is unmounted
///
/// when drop the [`MountHandle`], it will unmount Filesystem in background task, if user want to
/// wait unmount completely, use [`MountHandle::unmount`]
#[derive(Debug)]
pub struct MountHandle {
    inner: Option<MountHandleInner>,
}

impl MountHandle {
    pub async fn unmount(mut self) -> IoResult<()> {
        self.inner
            .take()
            .expect("unmount call twice")
            .inner_unmount()
            .await
    }
}

impl Drop for MountHandle {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            if inner.task.is_finished() {
                return;
            }

            #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
            {
                task::spawn(inner.inner_unmount()).detach();
            }

            #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
            {
                task::spawn(inner.inner_unmount());
            }
        }
    }
}

#[derive(Debug)]
struct MountHandleInner {
    task: JoinHandle<IoResult<()>>,
    mount_path: PathBuf,
    destroy_notify: Arc<async_notify::Notify>,
    #[cfg(any(
        all(target_os = "linux", feature = "unprivileged"),
        target_os = "macos"
    ))]
    unprivileged: bool,
}

impl MountHandleInner {
    async fn inner_unmount(self) -> IoResult<()> {
        self.destroy_notify.notify();

        #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
        {
            // wait destroy done
            self.task.await?;

            // TODO: freebsd mount is unprivileged, then unmount is unprivileged too?
            #[cfg(target_os = "freebsd")]
            {
                task::spawn_blocking(move || {
                    mount::unmount(&self.mount_path, MntFlags::MNT_SYNCHRONOUS)
                })
                .await?;
            }

            #[cfg(target_os = "macos")]
            {
                task::spawn_blocking(move || {
                    mount::unmount(&self.mount_path, MntFlags::MNT_SYNCHRONOUS)
                })
                .await?;
            }

            #[cfg(target_os = "linux")]
            {
                #[cfg(all(target_os = "linux", feature = "unprivileged"))]
                if self.unprivileged {
                    use std::io::ErrorKind;
                    let binary_path = find_fusermount3()?;
                    let mut child = Command::new(binary_path)
                        .args([OsStr::new("-u"), self.mount_path.as_os_str()])
                        .spawn()?;
                    if !child.status().await?.success() {
                        return Err(IoError::new(
                            ErrorKind::Other,
                            "call fusermount3 -u to unmount failed",
                        ));
                    }

                    return Ok(());
                }

                task::spawn_blocking(move || mount::umount(&self.mount_path)).await?;
            }
        }

        #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
        {
            // wait destroy done
            self.task.await.unwrap()?;

            // TODO: freebsd mount is unprivileged, then unmount is unprivileged too?
            #[cfg(target_os = "freebsd")]
            {
                task::spawn_blocking(move || {
                    mount::unmount(&self.mount_path, MntFlags::MNT_SYNCHRONOUS)
                })
                .await
                .unwrap()?;
            }
            #[cfg(target_os = "macos")]
            {
                task::spawn_blocking(move || {
                    mount::unmount(&self.mount_path, MntFlags::MNT_SYNCHRONOUS)
                })
                .await
                .unwrap()?;
            }

            #[cfg(target_os = "linux")]
            {
                #[cfg(all(target_os = "linux", feature = "unprivileged"))]
                if self.unprivileged {
                    let binary_path = find_fusermount3()?;
                    let mut child = Command::new(binary_path)
                        .args([OsStr::new("-u"), self.mount_path.as_os_str()])
                        .spawn()?;
                    if !child.wait().await?.success() {
                        return Err(IoError::other("call fusermount3 -u to unmount failed"));
                    }

                    return Ok(());
                }

                task::spawn_blocking(move || mount::umount(&self.mount_path))
                    .await
                    .unwrap()?;
            }
        }

        Ok(())
    }
}

impl Future for MountHandle {
    type Output = IoResult<()>;

    #[cfg(feature = "async-io-runtime")]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner.as_mut().expect("inner should be Some()").task).poll(cx)
    }

    #[cfg(feature = "tokio-runtime")]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // The unwrap is necessary in order to provide the same API for both runtimes, and actually
        // unwrap should not panic, when MountHandle is canceled by unmount method, user has no
        // chance to poll again
        Pin::new(&mut self.inner.as_mut().expect("inner should be Some()").task)
            .poll(cx)
            .map(Result::unwrap)
    }
}

#[cfg(any(feature = "async-io-runtime", feature = "tokio-runtime"))]
/// fuse filesystem session, inode based.
pub struct Session<FS: Filesystem + Send + Sync + 'static> {
    fuse_connection: Option<Arc<FuseConnection>>,
    filesystem: Option<Arc<FS>>,
    response_sender: UnboundedSender<FuseData>,
    response_receiver: Option<UnboundedReceiver<FuseData>>,
    mount_options: MountOptions,
    // ---- Concurrency configuration (currently unused wiring, will be utilized in later steps) ----
    /// Number of worker tasks to execute FUSE requests. 0 or 1 keeps legacy inline spawn behavior.
    worker_count: usize,
    /// Upper bound of in-flight (queued + running) requests before applying backpressure.
    max_background: usize,
    /// If true, serialize operations per (parent inode, name) / inode to preserve ordering.
    _per_inode_serial: bool,
    /// Internal worker pool (created lazily when worker_count > 1). Not yet wired into dispatch.
    workers: Option<Workers<FS>>,
    inflight: Arc<AtomicUsize>,
    inflight_notify: Arc<async_notify::Notify>,
}

#[derive(Debug)]
/// Represents a work item to be processed by a worker thread in the worker pool
pub(crate) struct WorkItem {
    pub(crate) unique: u64,
    opcode: u32,
    pub(crate) in_header: InHeaderLite,
    /// Body data (excludes fixed-size fuse_in_header)
    data: Vec<u8>,
    _inflight_guard: InflightGuard,
}

#[derive(Debug, Clone, Copy)]
/// Lightweight version of fuse_in_header containing essential fields
pub(crate) struct InHeaderLite {
    pub(crate) nodeid: u64,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) pid: u32,
}

#[derive(Debug)]
struct Workers<FS: Filesystem + Send + Sync + 'static> {
    /// Input queues for each worker
    senders: Vec<Sender<WorkItem>>,
    /// Round-robin counter for load balancing
    next: AtomicUsize,
    #[allow(dead_code)]
    handles: Vec<JoinHandle<()>>,
    _ctx: Arc<DispatchCtx<FS>>,
}
#[derive(Debug)]
/// RAII guard that tracks the number of in-flight requests
/// Increments counter on creation and decrements on drop
pub struct InflightGuard {
    inflight: Arc<AtomicUsize>,
    notify: Arc<async_notify::Notify>,
}

impl InflightGuard {
    pub fn new(inflight: Arc<AtomicUsize>, notify: Arc<async_notify::Notify>) -> Self {
        inflight.fetch_add(1, Ordering::AcqRel);
        Self { inflight, notify }
    }
}
impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.inflight.fetch_sub(1, Ordering::AcqRel);
        self.notify.notify();
    }
}

/// used for dispatching the request to the right worker handler
macro_rules! dispatch_to_worker {
    (
        match $target:expr, {
            ctx => $ctx:expr,
            worker_idx => $worker_idx:expr,
            item => $item:expr,

            $( $op:ident => $handler:ident, )*

            _ => { $($other_logic:tt)* }
        }
    ) => {
        match $target {
            $(
                Ok(fuse_opcode::$op) => {
                    debug!(
                        worker = %$worker_idx,
                        unique = $item.unique,
                        "worker handling {}",
                        stringify!($op).replace("FUSE_", "")
                    );
                    $handler($ctx, $item).await;
                },
            )*
            _ => { $($other_logic)* }
        }
    };
}

#[derive(Debug)]
struct DispatchCtx<FS: Filesystem + Send + Sync + 'static> {
    fs: Arc<FS>,
    resp: UnboundedSender<FuseData>,
    _inflight: Arc<AtomicUsize>,
    _inflight_notify: Arc<async_notify::Notify>,
}

impl<FS: Filesystem + Send + Sync + 'static> Workers<FS> {
    fn new(worker_count: usize, queue_capacity: usize, _ctx: Arc<DispatchCtx<FS>>) -> Self {
        let mut senders = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);
        for idx in 0..worker_count {
            let (tx, mut rx): (Sender<WorkItem>, Receiver<WorkItem>) = channel(queue_capacity);
            let ctx_clone = _ctx.clone();
            #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
            let handle = task::spawn(async move {
                while let Some(item) = rx.next().await {
                    process_work_item(&ctx_clone, idx, item).await;
                }
                debug!(worker=%idx, "worker exit");
            });
            #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
            let handle = task::spawn(async move {
                while let Some(item) = rx.next().await {
                    process_work_item(&ctx_clone, idx, item).await;
                }
                debug!(worker=%idx, "worker exit");
            });
            senders.push(tx);
            handles.push(handle);
        }
        Self {
            senders,
            next: AtomicUsize::new(0),
            handles,
            _ctx,
        }
    }

    async fn submit(&self, item: WorkItem) {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        // self.ctx.inflight.fetch_add(1, Ordering::AcqRel);
        if self.senders[idx].clone().send(item).await.is_err() {
            // failed to enqueue
            warn!("failed to enqueue work item, channel closed");
        }
    }
}

async fn process_work_item<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    worker_idx: usize,
    item: WorkItem,
) {
    use crate::raw::abi::fuse_opcode;
    let opcode_result = fuse_opcode::try_from(item.opcode);
    dispatch_to_worker! {
        match opcode_result, {
            ctx => ctx,
            worker_idx => worker_idx,
            item => item,
            FUSE_LOOKUP   => worker_lookup,
            FUSE_GETATTR  => worker_getattr,
            FUSE_OPEN     => worker_open,
            FUSE_READ     => worker_read,
            FUSE_WRITE    => worker_write,
            FUSE_READDIR  => worker_readdir,
            FUSE_SETATTR  => worker_setattr,
            FUSE_READLINK => worker_readlink,
            FUSE_SYMLINK  => worker_symlink,
            FUSE_MKNOD    => worker_mknod,
            FUSE_MKDIR    => worker_mkdir,
            FUSE_UNLINK   => worker_unlink,
            FUSE_RMDIR    => worker_rmdir,
            FUSE_RENAME   => worker_rename,
            FUSE_LINK     => worker_link,
            FUSE_STATFS   => worker_statfs,
            FUSE_RELEASE  => worker_release,
            FUSE_FSYNC    => worker_fsync,
            FUSE_SETXATTR => worker_setxattr,
            FUSE_GETXATTR => worker_getxattr,
            FUSE_LISTXATTR => worker_listxattr,
            FUSE_REMOVEXATTR => worker_removexattr,
            FUSE_FLUSH    => worker_flush,
            FUSE_OPENDIR => worker_opendir,
            FUSE_RELEASEDIR => worker_releasedir,
            FUSE_FSYNCDIR => worker_fsyncdir,
            FUSE_ACCESS  => worker_access,
            FUSE_CREATE  => worker_create,
            FUSE_BMAP    => worker_bmap,
            FUSE_FALLOCATE => worker_fallocate,
            FUSE_READDIRPLUS => worker_readdirplus,
            FUSE_RENAME2 => worker_rename2,
            FUSE_LSEEK => worker_lseek,
            FUSE_COPY_FILE_RANGE => worker_copy_file_range,
            FUSE_POLL => worker_poll,
            FUSE_BATCH_FORGET => worker_batch_forget,
            _ => {
                match opcode_result {
                    #[cfg(feature = "file-lock")]
                    Ok(fuse_opcode::FUSE_GETLK) => {
                        debug!(worker=%worker_idx, unique=item.unique, "worker handling GETLK");
                        worker_getlk(ctx, item).await;
                    }
                    #[cfg(feature = "file-lock")]
                    Ok(fuse_opcode::FUSE_SETLK | fuse_opcode::FUSE_SETLKW) => {
                        debug!(worker=%worker_idx, unique=item.unique, "worker handling SETLK/SETLKW");
                        let is_blocking = item.opcode == fuse_opcode::FUSE_SETLKW as u32;
                        worker_setlk(ctx, item, is_blocking).await;
                    }
                    #[cfg(target_os = "macos")]
                    Ok(fuse_opcode::FUSE_SETVOLNAME) => {
                        debug!(worker=%worker_idx, unique=item.unique, "worker handling SETVOLNAME");
                        worker_setvolname(ctx, item).await;
                    }
                    #[cfg(target_os = "macos")]
                    Ok(fuse_opcode::FUSE_GETXTIMES) => {
                        debug!(worker=%worker_idx, unique=item.unique, "worker handling GETXTIMES");
                        worker_getxtimes(ctx, item).await;
                    }
                    #[cfg(target_os = "macos")]
                    Ok(fuse_opcode::FUSE_EXCHANGE) => {
                        debug!(worker=%worker_idx, unique=item.unique, "worker handling EXCHANGE");
                        worker_exchange(ctx, item).await;
                    }
                    Ok(_) => {
                        debug!(worker=%worker_idx, unique=item.unique, opcode=item.opcode, "opcode not yet handled in worker");
                    }
                    Err(err) => {
                        debug!(worker=%worker_idx, unique=item.unique, raw=item.opcode, "unknown opcode {}", err.0);
                    }
                }
            }
        }
    }
}

#[inline]
fn reply_error_in_worker(err: Errno, item: &WorkItem) -> Result<Vec<u8>, Box<bincode::ErrorKind>> {
    let out_header = fuse_out_header {
        len: FUSE_OUT_HEADER_SIZE as u32,
        error: err.into(),
        unique: item.unique,
    };
    get_bincode_config().serialize(&out_header)
}
async fn worker_lookup<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let name = match get_first_null_position(&item.data) {
        None => {
            debug!(unique = item.unique, "lookup body has no null (worker)");
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(idx) => OsString::from_vec(item.data[..idx].to_vec()),
    };
    let parent = item.in_header.nodeid;
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_lookup_worker"), async move {
        debug!(unique = item.unique, parent, ?name, "lookup (worker)");
        let data = match fs.lookup(Request::from(&item), parent, &name).await {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(entry) => {
                let entry_out: fuse_entry_out = entry.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                data
            }
        };
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_getattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let getattr_in = match get_bincode_config().deserialize::<fuse_getattr_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_getattr_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fh = if getattr_in.getattr_flags & FUSE_GETATTR_FH > 0 {
        Some(getattr_in.fh)
    } else {
        None
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_getattr_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            "getattr (worker)"
        );
        let data = match fs
            .getattr(
                Request::from(&item),
                item.in_header.nodeid,
                fh,
                getattr_in.getattr_flags,
            )
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(attr) => {
                let attr_out = fuse_attr_out {
                    attr_valid: attr.ttl.as_secs(),
                    attr_valid_nsec: attr.ttl.subsec_nanos(),
                    dummy: getattr_in.dummy,
                    attr: attr.attr.into(),
                };
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &attr_out)
                    .expect("serialize attr_out");
                data
            }
        };
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_open<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let open_in = match get_bincode_config().deserialize::<fuse_open_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_open_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp = ctx.resp.clone();
    spawn(debug_span!("fuse_open_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            flags = open_in.flags,
            "open (worker)"
        );
        let data = match fs
            .open(Request::from(&item), item.in_header.nodeid, open_in.flags)
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(opened) => {
                let open_out: fuse_open_out = opened.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &open_out)
                    .expect("serialize open_out");
                data
            }
        };
        let _ = resp.unbounded_send(Either::Left(data));
    });
}

async fn worker_read<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let read_in = match get_bincode_config().deserialize::<fuse_read_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_read_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp = ctx.resp.clone();
    spawn(debug_span!("fuse_read_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            size = read_in.size,
            offset = read_in.offset,
            "read (worker)"
        );
        let mut reply_data = match fs
            .read(
                Request::from(&item),
                item.in_header.nodeid,
                read_in.fh,
                read_in.offset,
                read_in.size,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp.unbounded_send(Either::Left(data));
                return;
            }
            Ok(reply) => reply.data,
        };
        if reply_data.len() > read_in.size as usize {
            reply_data.truncate(read_in.size as usize);
        }
        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + reply_data.len()) as u32,
            error: 0,
            unique: item.unique,
        };
        let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
        get_bincode_config()
            .serialize_into(&mut data_buf, &out_header)
            .expect("serialize header");
        let _ = resp.unbounded_send(Either::Right((data_buf, reply_data)));
    });
}

async fn worker_write<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    if item.data.len() < FUSE_WRITE_IN_SIZE {
        // malformed
        let data = reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
        let _ = ctx.resp.unbounded_send(Either::Left(data));
        return;
    }
    let write_in =
        match get_bincode_config().deserialize::<fuse_write_in>(&item.data[..FUSE_WRITE_IN_SIZE]) {
            Err(err) => {
                debug!(
                    unique = item.unique,
                    "deserialize fuse_write_in failed {}", err
                );
                let data = reply_error_in_worker(libc::EINVAL.into(), &item)
                    .expect("serialize out_header");
                let _ = ctx.resp.unbounded_send(Either::Left(data));
                return;
            }
            Ok(v) => v,
        };
    let payload = &item.data[FUSE_WRITE_IN_SIZE..];
    if write_in.size as usize != payload.len() {
        let data = reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
        let _ = ctx.resp.unbounded_send(Either::Left(data));
        return;
    }
    let data_vec = payload.to_vec();
    let fs = ctx.fs.clone();
    let resp = ctx.resp.clone();
    spawn(debug_span!("fuse_write_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            size = write_in.size,
            offset = write_in.offset,
            "write (worker)"
        );
        let write_out_data = match fs
            .write(
                Request::from(&item),
                item.in_header.nodeid,
                write_in.fh,
                write_in.offset,
                &data_vec,
                write_in.write_flags,
                write_in.flags,
            )
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(reply_write) => {
                let write_out: fuse_write_out = reply_write.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &write_out)
                    .expect("serialize write_out");
                data
            }
        };
        let _ = resp.unbounded_send(Either::Left(write_out_data));
    });
}

async fn worker_readdir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    // need mount options to check force_readdir_plus; currently not in ctx, so just execute kernel-side ENOSYS logic inline here is impossible.
    // For now we optimistically proceed; if force_readdir_plus was set, kernel shouldn't send READDIR anyway (original code returned ENOSYS).
    let read_in = match get_bincode_config().deserialize::<fuse_read_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_read_in (readdir) failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp = ctx.resp.clone();
    spawn(debug_span!("fuse_readdir_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = read_in.fh,
            offset = read_in.offset,
            "readdir (worker)"
        );
        let reply_readdir = match fs
            .readdir(
                Request::from(&item),
                item.in_header.nodeid,
                read_in.fh,
                read_in.offset as i64,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp.unbounded_send(Either::Left(data));
                return;
            }
            Ok(r) => r,
        };
        let max_size = read_in.size as usize;
        let mut entry_data = Vec::with_capacity(max_size);
        let mut entries = pin!(reply_readdir.entries);
        while let Some(entry) = entries.next().await {
            let entry = match entry {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: item.unique,
                    };
                    let data = get_bincode_config()
                        .serialize(&out_header)
                        .expect("serialize out_header");
                    let _ = resp.unbounded_send(Either::Left(data));
                    return;
                }
                Ok(e) => e,
            };
            let name = &entry.name;
            let dir_entry_size = FUSE_DIRENT_SIZE + name.len();
            let padding_size = get_padding_size(dir_entry_size);
            if entry_data.len() + dir_entry_size > max_size {
                break;
            }
            let dir_entry = fuse_dirent {
                ino: entry.inode,
                off: entry.offset as u64,
                namelen: name.len() as u32,
                r#type: mode_from_kind_and_perm(entry.kind, 0) >> 12,
            };
            get_bincode_config()
                .serialize_into(&mut entry_data, &dir_entry)
                .expect("serialize dirent");
            entry_data.extend_from_slice(name.as_bytes());
            entry_data.resize(entry_data.len() + padding_size, 0);
        }
        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
            error: 0,
            unique: item.unique,
        };
        let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
        get_bincode_config()
            .serialize_into(&mut data_buf, &out_header)
            .expect("serialize header");
        let _ = resp.unbounded_send(Either::Right((data_buf, entry_data.into())));
    });
}

async fn worker_setattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let setattr_in = match get_bincode_config().deserialize::<fuse_setattr_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_setattr_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_setattr_worker"), async move {
        let set_attr = SetAttr::from(&setattr_in);
        let fh = if setattr_in.valid & FATTR_FH > 0 {
            Some(setattr_in.fh)
        } else {
            None
        };
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            "setattr (worker)"
        );
        let data = match fs
            .setattr(Request::from(&item), item.in_header.nodeid, fh, set_attr)
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(attr) => {
                let attr_out: fuse_attr_out = attr.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &attr_out)
                    .expect("serialize attr_out");
                data
            }
        };
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_readlink<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_readlink_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            "readlink (worker)"
        );
        let data = match fs
            .readlink(Request::from(&item), item.in_header.nodeid)
            .await
        {
            Err(err) => {
                let out_header = fuse_out_header {
                    len: FUSE_OUT_HEADER_SIZE as u32,
                    error: err.into(),
                    unique: item.unique,
                };
                Either::Left(
                    get_bincode_config()
                        .serialize(&out_header)
                        .expect("serialize out_header"),
                )
            }
            Ok(data) => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + data.data.len()) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data_buf, &out_header)
                    .expect("serialize header");
                Either::Right((data_buf, data.data))
            }
        };
        let _ = resp_sender.unbounded_send(data);
    });
}

async fn worker_symlink<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let (name, first_null_index) = match get_first_null_position(&item.data) {
        None => {
            debug!(unique = item.unique, "symlink has no null (worker)");
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => (OsString::from_vec(item.data[..index].to_vec()), index),
    };
    let data = &item.data[first_null_index + 1..];
    let link_name = match get_first_null_position(data) {
        None => {
            debug!(unique = item.unique, "symlink has no second null (worker)");
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };
    let parent = item.in_header.nodeid;
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_symlink_worker"), async move {
        debug!(
            unique = item.unique,
            parent,
            ?name,
            ?link_name,
            "symlink (worker)"
        );
        let data = match fs
            .symlink(Request::from(&item), parent, &name, &link_name)
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(entry) => {
                let entry_out: fuse_entry_out = entry.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                data
            }
        };
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_mknod<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let mknod_in = match get_bincode_config().deserialize::<fuse_mknod_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_mknod_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let data = &item.data[FUSE_MKNOD_IN_SIZE..];
    let name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_mknod_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_mknod_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            ?mknod_in,
            "mknod (worker)"
        );
        match fs
            .mknod(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                mknod_in.mode,
                mknod_in.rdev,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(entry) => {
                let entry_out: fuse_entry_out = entry.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_mkdir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let mkdir_in = match get_bincode_config().deserialize::<fuse_mkdir_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_mkdir_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let data = &item.data[FUSE_MKDIR_IN_SIZE..];
    let name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_mkdir_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_mkdir_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            ?mkdir_in,
            "mkdir (worker)"
        );
        match fs
            .mkdir(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                mkdir_in.mode,
                mkdir_in.umask,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(entry) => {
                let entry_out: fuse_entry_out = entry.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_unlink<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let name = match get_first_null_position(&item.data) {
        None => {
            debug!(
                unique = item.unique,
                "unlink body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(item.data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_unlink_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            "unlink (worker)"
        );
        let resp_value = if let Err(err) = fs
            .unlink(Request::from(&item), item.in_header.nodeid, &name)
            .await
        {
            err.into()
        } else {
            0
        };
        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_rmdir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let name = match get_first_null_position(&item.data) {
        None => {
            debug!(
                unique = item.unique,
                "rmdir body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(item.data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_rmdir_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            "rmdir (worker)"
        );
        let resp_value = if let Err(err) = fs
            .rmdir(Request::from(&item), item.in_header.nodeid, &name)
            .await
        {
            err.into()
        } else {
            0
        };
        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_rename<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let rename_in = match get_bincode_config().deserialize::<fuse_rename_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_rename_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let mut data = &item.data[FUSE_RENAME_IN_SIZE..];
    let (name, first_null_index) = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_rename_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
    };
    data = &data[first_null_index + 1..];
    let new_name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_rename_in body doesn't have second null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_rename_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            new_parent = rename_in.newdir,
            ?new_name,
            "rename (worker)"
        );
        let resp_value = if let Err(err) = fs
            .rename(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                rename_in.newdir,
                &new_name,
            )
            .await
        {
            err.into()
        } else {
            0
        };
        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_link<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let link_in = match get_bincode_config().deserialize::<fuse_link_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_link_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let data = &item.data[FUSE_LINK_IN_SIZE..];
    let name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_link_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_link_worker"), async move {
        debug!(
            unique = item.unique,
            inode = link_in.oldnodeid,
            new_parent = item.in_header.nodeid,
            ?name,
            "link (worker)"
        );
        match fs
            .link(
                Request::from(&item),
                link_in.oldnodeid,
                item.in_header.nodeid,
                &name,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(entry) => {
                let entry_out: fuse_entry_out = entry.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_statfs<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_statfs_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            "statfs (worker)"
        );
        let fs_stat = match fs.statfs(Request::from(&item), item.in_header.nodeid).await {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
                return;
            }
            Ok(fs_stat) => fs_stat,
        };
        let statfs_out: fuse_statfs_out = fs_stat.into();
        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE) as u32,
            error: 0,
            unique: item.unique,
        };
        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);
        get_bincode_config()
            .serialize_into(&mut data, &out_header)
            .expect("serialize header");
        get_bincode_config()
            .serialize_into(&mut data, &statfs_out)
            .expect("serialize statfs_out");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_release<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let release_in = match get_bincode_config().deserialize::<fuse_release_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_release_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_release_worker"), async move {
        let flush = release_in.release_flags & FUSE_RELEASE_FLUSH > 0;
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = release_in.fh,
            flags = release_in.flags,
            lock_owner = release_in.lock_owner,
            flush,
            "release (worker)"
        );
        let resp_value = if let Err(err) = fs
            .release(
                Request::from(&item),
                item.in_header.nodeid,
                release_in.fh,
                release_in.flags,
                release_in.lock_owner,
                flush,
            )
            .await
        {
            err.into()
        } else {
            0
        };
        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_fsync<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let fsync_in = match get_bincode_config().deserialize::<fuse_fsync_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_fsync_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();
    spawn(debug_span!("fuse_fsync_worker"), async move {
        let data_sync = fsync_in.fsync_flags & 1 > 0;
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = fsync_in.fh,
            data_sync,
            "fsync (worker)"
        );
        let resp_value = if let Err(err) = fs
            .fsync(
                Request::from(&item),
                item.in_header.nodeid,
                fsync_in.fh,
                data_sync,
            )
            .await
        {
            err.into()
        } else {
            0
        };
        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_setxattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let setxattr_in = match get_bincode_config().deserialize::<fuse_setxattr_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_setxattr_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let mut data = &item.data[FUSE_SETXATTR_IN_SIZE..];
    let (name, first_null_index) = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_setxattr_in body doesn't have null"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
    };

    data = &data[first_null_index + 1..];
    if data.len() < setxattr_in.size as usize {
        debug!(unique = item.unique, "setxattr value data too short");
        let data = reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
        let _ = ctx.resp.unbounded_send(Either::Left(data));
        return;
    }

    let value = data[..setxattr_in.size as usize].to_vec();
    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_setxattr_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            ?name,
            size = setxattr_in.size,
            flags = setxattr_in.flags,
            "setxattr (worker)"
        );
        // TODO handle os X argument
        let resp_value = if let Err(err) = fs
            .setxattr(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                &value,
                setxattr_in.flags,
                #[cfg(target_os = "macos")]
                setxattr_in.position,
                #[cfg(not(target_os = "macos"))]
                0,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_getxattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let getxattr_in = match get_bincode_config().deserialize::<fuse_getxattr_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_getxattr_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let data = &item.data[FUSE_GETXATTR_IN_SIZE..];
    let name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_getxattr_in body doesn't have null"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_getxattr_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            ?name,
            size = getxattr_in.size,
            "getxattr (worker)"
        );

        let reply_result = fs
            .getxattr(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                getxattr_in.size,
            )
            .await;

        match reply_result {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(reply_xattr) => match reply_xattr {
                ReplyXAttr::Size(size) => {
                    let getxattr_out = fuse_getxattr_out { size, _padding: 0 };
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                        error: libc::ERANGE,
                        unique: item.unique,
                    };
                    let mut data =
                        Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE);
                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("serialize header");
                    get_bincode_config()
                        .serialize_into(&mut data, &getxattr_out)
                        .expect("serialize getxattr_out");
                    let _ = resp_sender.unbounded_send(Either::Left(data));
                }
                ReplyXAttr::Data(data_vec) => {
                    // TODO check is right way or not
                    // TODO should we check data length or not
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + data_vec.len()) as u32,
                        error: 0,
                        unique: item.unique,
                    };
                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("serialize header");
                    data.extend_from_slice(&data_vec);
                    let _ = resp_sender.unbounded_send(Either::Right((data, data_vec)));
                }
            },
        };
    });
}

async fn worker_listxattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let getxattr_in = match get_bincode_config().deserialize::<fuse_getxattr_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_getxattr_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_listxattr_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            size = getxattr_in.size,
            "listxattr (worker)"
        );

        let reply_result = fs
            .listxattr(
                Request::from(&item),
                item.in_header.nodeid,
                getxattr_in.size,
            )
            .await;

        match reply_result {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(reply_xattr) => match reply_xattr {
                ReplyXAttr::Size(size) => {
                    let getxattr_out = fuse_getxattr_out { size, _padding: 0 };
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                        error: 0, //almost as same as getxattr.did the error right?
                        unique: item.unique,
                    };
                    let mut data =
                        Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE);
                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("serialize header");
                    get_bincode_config()
                        .serialize_into(&mut data, &getxattr_out)
                        .expect("serialize getxattr_out");
                    let _ = resp_sender.unbounded_send(Either::Left(data));
                }
                ReplyXAttr::Data(data_vec) => {
                    // TODO check is right way or not
                    // TODO should we check data length or not
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + data_vec.len()) as u32,
                        error: 0,
                        unique: item.unique,
                    };
                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("serialize header");
                    data.extend_from_slice(&data_vec);
                    let _ = resp_sender.unbounded_send(Either::Right((data, data_vec)));
                }
            },
        };
    });
}

async fn worker_removexattr<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let name = match get_first_null_position(&item.data) {
        None => {
            debug!(
                unique = item.unique,
                "removexattr body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(item.data[..index].to_vec()),
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_removexattr_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            ?name,
            "removexattr (worker)"
        );

        let resp_value = if let Err(err) = fs
            .removexattr(Request::from(&item), item.in_header.nodeid, &name)
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_flush<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let flush_in = match get_bincode_config().deserialize::<fuse_flush_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_flush_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_flush_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = flush_in.fh,
            lock_owner = flush_in.lock_owner,
            "flush (worker)"
        );

        let resp_value = if let Err(err) = fs
            .flush(
                Request::from(&item),
                item.in_header.nodeid,
                flush_in.fh,
                flush_in.lock_owner,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_opendir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let open_in = match get_bincode_config().deserialize::<fuse_open_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_open_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_opendir_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            flags = open_in.flags,
            "opendir (worker)"
        );

        let data = match fs
            .opendir(Request::from(&item), item.in_header.nodeid, open_in.flags)
            .await
        {
            Err(err) => reply_error_in_worker(err, &item).expect("serialize out_header"),
            Ok(opened) => {
                let open_out: fuse_open_out = opened.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &open_out)
                    .expect("serialize open_out");
                data
            }
        };

        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_releasedir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let release_in = match get_bincode_config().deserialize::<fuse_release_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_release_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_releasedir_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = release_in.fh,
            flags = release_in.flags,
            "releasedir (worker)"
        );

        let resp_value = if let Err(err) = fs
            .releasedir(
                Request::from(&item),
                item.in_header.nodeid,
                release_in.fh,
                release_in.flags,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_fsyncdir<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let fsync_in = match get_bincode_config().deserialize::<fuse_fsync_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_fsync_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_fsyncdir_worker"), async move {
        let data_sync = fsync_in.fsync_flags & 1 > 0;
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = fsync_in.fh,
            data_sync,
            "fsyncdir (worker)"
        );

        let resp_value = if let Err(err) = fs
            .fsyncdir(
                Request::from(&item),
                item.in_header.nodeid,
                fsync_in.fh,
                data_sync,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_access<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let access_in = match get_bincode_config().deserialize::<fuse_access_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_access_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_access_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            mask = access_in.mask,
            "access (worker)"
        );

        let resp_value = if let Err(err) = fs
            .access(Request::from(&item), item.in_header.nodeid, access_in.mask)
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_create<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let create_in = match get_bincode_config().deserialize::<fuse_create_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_create_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let data = &item.data[FUSE_CREATE_IN_SIZE..];
    let name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_create_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_create_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            mode = create_in.mode,
            flags = create_in.flags,
            "create (worker)"
        );

        match fs
            .create(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                create_in.mode,
                create_in.flags,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(created) => {
                let (entry_out, open_out): (fuse_entry_out, fuse_open_out) = created.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(
                    FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE,
                );
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &entry_out)
                    .expect("serialize entry");
                get_bincode_config()
                    .serialize_into(&mut data, &open_out)
                    .expect("serialize open");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_bmap<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let bmap_in = match get_bincode_config().deserialize::<fuse_bmap_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_bmap_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_bmap_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            blocksize = bmap_in.blocksize,
            block = bmap_in.block,
            "bmap (worker)"
        );

        match fs
            .bmap(
                Request::from(&item),
                item.in_header.nodeid,
                bmap_in.blocksize,
                bmap_in.block,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(bmap_reply) => {
                let bmap_out: fuse_bmap_out = bmap_reply.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &bmap_out)
                    .expect("serialize bmap_out");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_fallocate<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let fallocate_in = match get_bincode_config().deserialize::<fuse_fallocate_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_fallocate_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_fallocate_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = fallocate_in.fh,
            offset = fallocate_in.offset,
            length = fallocate_in.length,
            mode = fallocate_in.mode,
            "fallocate (worker)"
        );

        let resp_value = if let Err(err) = fs
            .fallocate(
                Request::from(&item),
                item.in_header.nodeid,
                fallocate_in.fh,
                fallocate_in.offset,
                fallocate_in.length,
                fallocate_in.mode,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_readdirplus<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let read_in = match get_bincode_config().deserialize::<fuse_read_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_read_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_readdirplus_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = read_in.fh,
            offset = read_in.offset,
            "readdirplus (worker)"
        );

        let reply_readdir_plus = match fs
            .readdirplus(
                Request::from(&item),
                item.in_header.nodeid,
                read_in.fh,
                read_in.offset,
                read_in.lock_owner,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
                return;
            }
            Ok(r) => r,
        };

        let max_size = read_in.size as usize;
        let mut entry_data = Vec::with_capacity(max_size);
        let mut entries = pin!(reply_readdir_plus.entries);

        while let Some(entry_plus) = entries.next().await {
            let entry_plus = match entry_plus {
                Err(err) => {
                    let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                    let _ = resp_sender.unbounded_send(Either::Left(data));
                    return;
                }
                Ok(entry_plus) => entry_plus,
            };

            let name = &entry_plus.name;
            let dirent_plus_size = FUSE_DIRENTPLUS_SIZE + name.len();
            let padding_size = get_padding_size(dirent_plus_size);

            if entry_data.len() + dirent_plus_size > max_size {
                break;
            }

            let entry_out = fuse_entry_out {
                nodeid: entry_plus.attr.ino,
                generation: entry_plus.generation,
                entry_valid: entry_plus.entry_ttl.as_secs(),
                attr_valid: entry_plus.attr_ttl.as_secs(),
                entry_valid_nsec: entry_plus.entry_ttl.subsec_nanos(),
                attr_valid_nsec: entry_plus.attr_ttl.subsec_nanos(),
                attr: entry_plus.attr.into(),
            };
            let dirent_plus = fuse_direntplus {
                entry_out,
                dirent: fuse_dirent {
                    ino: entry_plus.inode,
                    off: entry_plus.offset as u64,
                    namelen: name.len() as u32,
                    // learn from fuse-rs and golang bazil.org fuse DirentType
                    r#type: mode_from_kind_and_perm(entry_plus.kind, 0) >> 12,
                },
            };

            get_bincode_config()
                .serialize_into(&mut entry_data, &dirent_plus)
                .expect("serialize direntplus");
            entry_data.extend_from_slice(name.as_bytes());
            entry_data.resize(entry_data.len() + padding_size, 0);
        }

        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
            error: 0,
            unique: item.unique,
        };
        let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);
        get_bincode_config()
            .serialize_into(&mut data_buf, &out_header)
            .expect("serialize header");
        let _ = resp_sender.unbounded_send(Either::Right((data_buf, entry_data.into())));
    });
}

async fn worker_rename2<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let rename2_in = match get_bincode_config().deserialize::<fuse_rename2_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_rename2_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let mut data = &item.data[FUSE_RENAME2_IN_SIZE..];
    let (name, first_null_index) = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_rename2_in body doesn't have null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
    };

    data = &data[first_null_index + 1..];
    let new_name = match get_first_null_position(data) {
        None => {
            debug!(
                unique = item.unique,
                "fuse_rename2_in body doesn't have second null (worker)"
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Some(index) => OsString::from_vec(data[..index].to_vec()),
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_rename2_worker"), async move {
        debug!(
            unique = item.unique,
            parent = item.in_header.nodeid,
            ?name,
            new_parent = rename2_in.newdir,
            ?new_name,
            flags = rename2_in.flags,
            "rename2 (worker)"
        );

        let resp_value = if let Err(err) = fs
            .rename2(
                Request::from(&item),
                item.in_header.nodeid,
                &name,
                rename2_in.newdir,
                &new_name,
                rename2_in.flags,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_lseek<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let lseek_in = match get_bincode_config().deserialize::<fuse_lseek_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_lseek_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_lseek_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = lseek_in.fh,
            offset = lseek_in.offset,
            whence = lseek_in.whence,
            "lseek (worker)"
        );

        match fs
            .lseek(
                Request::from(&item),
                item.in_header.nodeid,
                lseek_in.fh,
                lseek_in.offset,
                lseek_in.whence,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(offset) => {
                let lseek_out = fuse_lseek_out {
                    offset: offset.offset,
                };
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_LSEEK_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_LSEEK_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &lseek_out)
                    .expect("serialize lseek_out");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

async fn worker_copy_file_range<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let copy_file_range_in =
        match get_bincode_config().deserialize::<fuse_copy_file_range_in>(&item.data) {
            Err(err) => {
                debug!(
                    unique = item.unique,
                    "deserialize fuse_copy_file_range_in failed {}", err
                );
                let data = reply_error_in_worker(libc::EINVAL.into(), &item)
                    .expect("serialize out_header");
                let _ = ctx.resp.unbounded_send(Either::Left(data));
                return;
            }
            Ok(v) => v,
        };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_copy_file_range_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh_in = copy_file_range_in.fh_in,
            off_in = copy_file_range_in.off_in,
            nodeid_out = copy_file_range_in.nodeid_out,
            fh_out = copy_file_range_in.fh_out,
            off_out = copy_file_range_in.off_out,
            len = copy_file_range_in.len,
            flags = copy_file_range_in.flags,
            "copy_file_range (worker)"
        );

        match fs
            .copy_file_range(
                Request::from(&item),
                item.in_header.nodeid,
                copy_file_range_in.fh_in,
                copy_file_range_in.off_in,
                copy_file_range_in.nodeid_out,
                copy_file_range_in.fh_out,
                copy_file_range_in.off_out,
                copy_file_range_in.len,
                copy_file_range_in.flags,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(copied) => {
                let copy_file_range_out: fuse_write_out = copied.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &copy_file_range_out)
                    .expect("serialize copy_file_range_out");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

#[cfg(feature = "file-lock")]
async fn worker_getlk<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let lk_in = match get_bincode_config().deserialize::<fuse_lk_in>(&item.data) {
        Err(err) => {
            debug!(
                unique = item.unique,
                "deserialize fuse_lk_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_getlk_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = lk_in.fh,
            owner = lk_in.owner,
            "getlk (worker)"
        );

        match fs
            .getlk(
                Request::from(&item),
                item.in_header.nodeid,
                lk_in.fh,
                lk_in.owner,
                lk_in.lk.start,
                lk_in.lk.end,
                lk_in.lk.typ,
                lk_in.lk.pid,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
            Ok(lock) => {
                let lk_out: fuse_lk_out = lock.into();
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE) as u32,
                    error: 0,
                    unique: item.unique,
                };
                let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE);
                get_bincode_config()
                    .serialize_into(&mut data, &out_header)
                    .expect("serialize header");
                get_bincode_config()
                    .serialize_into(&mut data, &lk_out)
                    .expect("serialize lk_out");
                let _ = resp_sender.unbounded_send(Either::Left(data));
            }
        }
    });
}

#[cfg(feature = "file-lock")]
async fn worker_setlk<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
    is_blocking: bool,
) {
    let lk_in = match get_bincode_config().deserialize::<fuse_lk_in>(&item.data) {
        Err(err) => {
            let opcode = if is_blocking {
                fuse_opcode::FUSE_SETLKW
            } else {
                fuse_opcode::FUSE_SETLK
            };

            error!(
                "deserialize fuse_lk_in in {:?} failed {}, request unique {}",
                opcode, err, item.unique
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_setlk_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = lk_in.fh,
            owner = lk_in.owner,
            is_blocking,
            "setlk (worker)"
        );

        let resp_value = if let Err(err) = fs
            .setlk(
                Request::from(&item),
                item.in_header.nodeid,
                lk_in.fh,
                lk_in.owner,
                lk_in.lk.start,
                lk_in.lk.end,
                lk_in.lk.typ,
                lk_in.lk.pid,
                is_blocking,
            )
            .await
        {
            err.into()
        } else {
            0
        };

        let data = reply_error_in_worker(resp_value.into(), &item).expect("serialize out_header");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_poll<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let poll_in = match get_bincode_config().deserialize::<fuse_poll_in>(&item.data) {
        Err(err) => {
            error!(
                unique = item.unique,
                "deserialize fuse_poll_in failed {}", err
            );
            let data =
                reply_error_in_worker(libc::EINVAL.into(), &item).expect("serialize out_header");
            let _ = ctx.resp.unbounded_send(Either::Left(data));
            return;
        }
        Ok(v) => v,
    };

    let fs = ctx.fs.clone();
    let resp_sender = ctx.resp.clone();

    spawn(debug_span!("fuse_poll_worker"), async move {
        debug!(
            unique = item.unique,
            inode = item.in_header.nodeid,
            fh = poll_in.fh,
            kh = poll_in.kh,
            flags = poll_in.flags,
            "poll (worker)"
        );

        let notify = Notify::new(resp_sender.clone());
        let reply_poll = match fs
            .poll(
                Request::from(&item),
                item.in_header.nodeid,
                poll_in.fh,
                if poll_in.flags & FUSE_POLL_SCHEDULE_NOTIFY == 0 {
                    None
                } else {
                    Some(poll_in.kh)
                },
                poll_in.flags,
                poll_in.events,
                &notify,
            )
            .await
        {
            Err(err) => {
                let data = reply_error_in_worker(err, &item).expect("serialize out_header");
                let _ = resp_sender.unbounded_send(Either::Left(data));
                return;
            }
            Ok(r) => r,
        };

        let poll_out: fuse_poll_out = reply_poll.into();

        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE) as u32,
            error: 0,
            unique: item.unique,
        };

        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE);
        get_bincode_config()
            .serialize_into(&mut data, &out_header)
            .expect("serialize header");
        get_bincode_config()
            .serialize_into(&mut data, &poll_out)
            .expect("serialize poll_out");
        let _ = resp_sender.unbounded_send(Either::Left(data));
    });
}

async fn worker_batch_forget<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
    let batch_forget_in = match get_bincode_config().deserialize::<fuse_batch_forget_in>(&item.data)
    {
        Err(err) => {
            error!(
                "deserialize fuse_batch_forget_in failed {}, request unique {}",
                err, item.unique
            );
            // batch_forget has no reply
            return;
        }
        Ok(v) => v,
    };

    let mut data = &item.data[FUSE_BATCH_FORGET_IN_SIZE..];
    let mut inodes = Vec::with_capacity(batch_forget_in.count as usize);

    for _ in 0..batch_forget_in.count {
        if data.len() < FUSE_FORGET_ONE_SIZE {
            error!(unique = item.unique, "batch_forget data too short");
            return;
        }

        let forget_one = match get_bincode_config()
            .deserialize::<fuse_forget_one>(&data[..FUSE_FORGET_ONE_SIZE])
        {
            Err(err) => {
                error!("deserialize fuse_batch_forget_in body fuse_forget_one failed {}, request unique {}", err, item.unique);
                // no need to reply
                return;
            }
            Ok(v) => v,
        };

        inodes.push((forget_one.nodeid, forget_one._nlookup));
        data = &data[FUSE_FORGET_ONE_SIZE..];
    }

    if inodes.len() != batch_forget_in.count as usize {
        error!(
            "fuse_forget_one count != fuse_batch_forget_in.count, request unique {}",
            item.unique
        );

        return;
    }

    let fs = ctx.fs.clone();

    spawn(debug_span!("fuse_batch_forget_worker"), async move {
        debug!(
            unique = item.unique,
            count = batch_forget_in.count,
            "batch_forget (worker)"
        );

        fs.batch_forget(
            Request {
                unique: item.unique,
                uid: item.in_header.uid,
                gid: item.in_header.gid,
                pid: item.in_header.pid,
            },
            &inodes,
        )
        .await;
        // batch_forget has no reply
    });
}

#[cfg(target_os = "macos")]
async fn worker_setvolname<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
}

#[cfg(target_os = "macos")]
async fn worker_getxtimes<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
}

#[cfg(target_os = "macos")]
async fn worker_exchange<FS: Filesystem + Send + Sync + 'static>(
    ctx: &Arc<DispatchCtx<FS>>,
    item: WorkItem,
) {
}

enum ReadResult {
    Destroy,
    Request {
        in_header: IoResult<fuse_in_header>,
        header_buffer: Vec<u8>,
        data_buffer: Vec<u8>,
    },
}

impl Debug for ReadResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadResult::Destroy => f.debug_struct("ReadResult::Destroy").finish(),
            ReadResult::Request { in_header, .. } => f
                .debug_struct("ReadResult::Request")
                .field("in_header", in_header)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(any(feature = "async-io-runtime", feature = "tokio-runtime"))]
impl<FS: Filesystem + Send + Sync + 'static> Session<FS> {
    /// new a fuse filesystem session.
    pub fn new(mount_options: MountOptions) -> Self {
        let (sender, receiver) = unbounded();

        Self {
            fuse_connection: None,
            filesystem: None,
            response_sender: sender,
            response_receiver: Some(receiver),
            mount_options,
            // default to legacy behaviour (no explicit pool)
            worker_count: 0,
            max_background: DEFAULT_MAX_BACKGROUND as usize,
            _per_inode_serial: false,
            workers: None,
            inflight: Arc::new(AtomicUsize::new(0)),
            inflight_notify: Arc::new(async_notify::Notify::new()),
        }
    }

    /// Configure worker parameters before mount (builder-style minimal helper).
    pub fn with_workers(mut self, worker_count: usize, max_background: usize) -> Self {
        self.worker_count = worker_count;
        self.max_background = max_background.max(1); // avoid zero
        self
    }

    fn ensure_workers(&mut self, fs: Arc<FS>) {
        if self.worker_count > 1 && self.workers.is_none() {
            let ctx = Arc::new(DispatchCtx {
                fs,
                resp: self.response_sender.clone(),
                _inflight: self.inflight.clone(),
                _inflight_notify: self.inflight_notify.clone(),
            });
            let workers = Workers::new(self.worker_count, self.max_background, ctx);
            self.workers = Some(workers);
            debug!(
                workers = self.worker_count,
                queue = self.max_background,
                "worker pool initialized"
            );
        }
    }

    /// get a [`notify`].
    ///
    /// [`notify`]: Notify
    fn get_notify(&self) -> Notify {
        Notify::new(self.response_sender.clone())
    }
}

#[cfg(any(feature = "async-io-runtime", feature = "tokio-runtime"))]
impl<FS: Filesystem + Send + Sync + 'static> Session<FS> {
    async fn mount_empty_check(&self, mount_path: &Path) -> IoResult<()> {
        use std::io::ErrorKind;

        #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
        if !self.mount_options.nonempty
            && matches!(read_dir(mount_path).await?.next_entry().await, Ok(Some(_)))
        {
            return Err(IoError::new(
                ErrorKind::AlreadyExists,
                "mount point is not empty",
            ));
        }

        #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
        if !self.mount_options.nonempty && read_dir(mount_path).await?.next().await.is_some() {
            return Err(IoError::new(
                ErrorKind::AlreadyExists,
                "mount point is not empty",
            ));
        }

        Ok(())
    }

    /// mount the filesystem without root permission. This function will block
    /// until the filesystem is unmounted.
    // On FreeBSD, no special interface is required to mount unprivileged.
    // If vfs.usermount=1 and the user has access to the mountpoint, it will
    // just work.
    #[cfg(all(target_os = "freebsd", feature = "unprivileged"))]
    pub async fn mount_with_unprivileged<P: AsRef<Path>>(
        self,
        fs: FS,
        mount_path: P,
    ) -> IoResult<MountHandle> {
        self.mount(fs, mount_path).await
    }

    #[cfg(target_os = "macos")]
    pub async fn mount_with_unprivileged<P: AsRef<Path>>(
        mut self,
        fs: FS,
        mount_path: P,
    ) -> IoResult<MountHandle> {
        let mount_path = mount_path.as_ref();

        self.mount_empty_check(mount_path).await?;

        let notify = Arc::new(async_notify::Notify::new());
        let fuse_connection = FuseConnection::new_with_unprivileged(
            self.mount_options.clone(),
            mount_path,
            notify.clone(),
        )
        .await?;

        self.fuse_connection.replace(Arc::new(fuse_connection));

        self.filesystem.replace(Arc::new(fs));

        debug!("mount {:?} success", mount_path);

        Ok(MountHandle {
            inner: Some(MountHandleInner {
                task: task::spawn(self.inner_mount()),
                mount_path: mount_path.to_path_buf(),
                destroy_notify: notify,
                unprivileged: true,
            }),
        })
    }

    /// mount the filesystem without root permission.
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    pub async fn mount_with_unprivileged<P: AsRef<Path>>(
        mut self,
        fs: FS,
        mount_path: P,
    ) -> IoResult<MountHandle> {
        let mount_path = mount_path.as_ref();

        self.mount_empty_check(mount_path).await?;

        let notify = Arc::new(async_notify::Notify::new());
        let fuse_connection = FuseConnection::new_with_unprivileged(
            self.mount_options.clone(),
            mount_path,
            notify.clone(),
        )
        .await?;

        self.fuse_connection.replace(Arc::new(fuse_connection));

        self.filesystem.replace(Arc::new(fs));

        debug!("mount {:?} success", mount_path);

        Ok(MountHandle {
            inner: Some(MountHandleInner {
                task: task::spawn(self.inner_mount()),
                mount_path: mount_path.to_path_buf(),
                destroy_notify: notify,
                unprivileged: true,
            }),
        })
    }

    /// mount the filesystem with root permission.
    #[cfg(target_os = "linux")]
    pub async fn mount<P: AsRef<Path>>(mut self, fs: FS, mount_path: P) -> IoResult<MountHandle> {
        let mount_path = mount_path.as_ref();

        self.mount_empty_check(mount_path).await?;

        let notify = Arc::new(async_notify::Notify::new());
        let fuse_connection = FuseConnection::new(notify.clone())?;

        let fd = fuse_connection.as_fd().as_raw_fd();

        let options = self.mount_options.build(fd);

        let fs_name = if let Some(fs_name) = self.mount_options.fs_name.as_ref() {
            Some(fs_name.as_str())
        } else {
            Some("fuse")
        };

        debug!("mount options {:?}", options);

        if let Err(err) = mount::mount(
            fs_name,
            mount_path,
            Some("fuse"),
            self.mount_options.flags(),
            Some(options.as_os_str()),
        ) {
            error!("mount {:?} failed", mount_path);

            return Err(err.into());
        }

        self.fuse_connection.replace(Arc::new(fuse_connection));

        self.filesystem.replace(Arc::new(fs));

        debug!("mount {:?} success", mount_path);

        Ok(MountHandle {
            inner: Some(MountHandleInner {
                task: task::spawn(self.inner_mount()),
                mount_path: mount_path.to_path_buf(),
                destroy_notify: notify,
                #[cfg(all(target_os = "linux", feature = "unprivileged"))]
                unprivileged: false,
            }),
        })
    }

    /// mount the filesystem
    #[cfg(target_os = "freebsd")]
    pub async fn mount<P: AsRef<Path>>(self, fs: FS, mount_path: P) -> IoResult<MountHandle> {
        let mount_path = mount_path.as_ref();

        self.mount_empty_check(mount_path).await?;

        let notify = Arc::new(async_notify::Notify::new());
        let fuse_connection = FuseConnection::new(notify.clone())?;

        let fd = fuse_connection.as_fd().as_raw_fd();

        {
            let mut nmount = self.mount_options.build();
            nmount
                .str_opt_owned(c"fspath", mount_path)
                .str_opt_owned(c"fd", format!("{}", fd).as_str());
            debug!("mount options {:?}", &nmount);

            if let Err(err) = nmount.nmount(self.mount_options.flags()) {
                error!("mount {} failed: {}", mount_path.display(), err);

                return Err(std::io::Error::from(err));
            }
        }

        self.fuse_connection.replace(Arc::new(fuse_connection));

        self.filesystem.replace(Arc::new(fs));

        debug!("mount {:?} success", mount_path);

        Ok(MountHandle {
            inner: Some(MountHandleInner {
                task: task::spawn(self.inner_mount()),
                mount_path: mount_path.to_path_buf(),
                destroy_notify: notify,
            }),
        })
    }

    #[cfg(target_os = "macos")]
    pub async fn mount<P: AsRef<Path>>(self, fs: FS, mount_path: P) -> IoResult<MountHandle> {
        self.mount_with_unprivileged(fs, mount_path).await
    }

    async fn inner_mount(mut self) -> IoResult<()> {
        let fuse_write_connection = self.fuse_connection.as_ref().unwrap().clone();

        let receiver = self.response_receiver.take().unwrap();

        let dispatch_task = self.dispatch().fuse();
        let mut dispatch_task = pin!(dispatch_task);

        #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
        let reply_task =
            task::spawn(async move { Self::reply_fuse(fuse_write_connection, receiver).await })
                .fuse();
        #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
        let reply_task = task::spawn(Self::reply_fuse(fuse_write_connection, receiver))
            .map(Result::unwrap)
            .fuse();

        let mut reply_task = pin!(reply_task);

        select! {
            reply_result = reply_task => {
                reply_result?;
            }

            dispatch_result = dispatch_task => {
                dispatch_result?;
            }
        }

        Ok(())
    }

    async fn reply_fuse(
        fuse_connection: Arc<FuseConnection>,
        mut response_receiver: UnboundedReceiver<FuseData>,
    ) -> IoResult<()> {
        while let Some(response) = response_receiver.next().await {
            let (data, extend_data) = match response {
                Either::Left(data) => (data, None),
                Either::Right((data, extend_data)) => (data, Some(extend_data)),
            };
            if let Err(err) = fuse_connection.write_vectored(data, extend_data).await.1 {
                use std::io::ErrorKind;
                if err.kind() == ErrorKind::NotFound {
                    warn!(
                        "may reply interrupted fuse request, ignore this error {}",
                        err
                    );

                    continue;
                }

                error!("reply fuse failed {}", err);

                return Err(err);
            }
        }

        Ok(())
    }

    #[instrument(level = "debug", skip(self, fs), ret, err)]
    async fn init_filesystem(
        &mut self,
        fs: &FS,
        fuse_connection: &FuseConnection,
    ) -> IoResult<NonZeroU32> {
        use std::io::ErrorKind;

        let header_buffer = vec![0; FUSE_IN_HEADER_SIZE];
        let data_buffer = vec![0; FUSE_MIN_READ_BUFFER_SIZE];

        let (data_buffer, in_header) = match self
            .read_fuse_request(fuse_connection, header_buffer, data_buffer)
            .await
        {
            ReadResult::Destroy => {
                return Err(IoError::new(
                    ErrorKind::UnexpectedEof,
                    "init stage get destroy result",
                ));
            }

            ReadResult::Request {
                in_header,
                data_buffer,
                ..
            } => {
                let in_header = in_header?;
                (data_buffer, in_header)
            }
        };

        let request = Request::from(&in_header);

        let opcode = match fuse_opcode::try_from(in_header.opcode) {
            Err(err) => {
                debug!("receive unknown opcode {}", err.0);

                reply_error_in_place(libc::ENOSYS.into(), request, &self.response_sender).await;

                return Err(IoError::other(format!("receive unknown opcode {}", err.0)));
            }

            Ok(opcode) => opcode,
        };

        debug!("receive opcode {}", opcode);

        if opcode != fuse_opcode::FUSE_INIT {
            error!(?opcode, "received unexpected opcode");

            return Err(IoError::other(format!("unexpected opcode {opcode:?}")));
        }

        let data_size = in_header.len as usize - FUSE_IN_HEADER_SIZE;
        let data_ref = &data_buffer[..data_size];

        self.handle_init(request, data_ref, fuse_connection, fs)
            .await
    }

    #[instrument(level = "debug", skip(self, header_buffer, data_buffer), ret)]
    async fn read_fuse_request(
        &mut self,
        fuse_connection: &FuseConnection,
        mut header_buffer: Vec<u8>,
        mut data_buffer: Vec<u8>,
    ) -> ReadResult {
        let res = match fuse_connection
            .read_vectored(header_buffer, data_buffer)
            .await
        {
            None => return ReadResult::Destroy,

            Some(((header_buf, data_buf), res)) => {
                header_buffer = header_buf;
                data_buffer = data_buf;

                res
            }
        };
        let n = match res {
            Err(err) => {
                if let Some(errno) = err.raw_os_error() {
                    if errno == libc::ENODEV {
                        debug!("read from /dev/fuse failed with ENODEV");

                        return ReadResult::Destroy;
                    }
                }

                error!("read from /dev/fuse failed {}", err);

                return ReadResult::Request {
                    in_header: Err(err),
                    header_buffer,
                    data_buffer,
                };
            }

            Ok(n) => n,
        };

        debug!(n, "read fuse request done");

        if n < FUSE_IN_HEADER_SIZE {
            error!(
                n,
                FUSE_IN_HEADER_SIZE, "read_vectored n is less then FUSE_IN_HEADER_SIZE"
            );

            return ReadResult::Request {
                in_header: Err(IoError::other(
                    "read_vectored n is less then FUSE_IN_HEADER_SIZE",
                )),
                header_buffer,
                data_buffer,
            };
        }

        let in_header = match get_bincode_config().deserialize::<fuse_in_header>(&header_buffer) {
            Err(err) => {
                error!("deserialize fuse_in_header failed {}", err);

                return ReadResult::Request {
                    in_header: Err(IoError::other(err)),
                    header_buffer,
                    data_buffer,
                };
            }

            Ok(in_header) => in_header,
        };

        ReadResult::Request {
            in_header: Ok(in_header),
            header_buffer,
            data_buffer,
        }
    }

    async fn dispatch(&mut self) -> IoResult<()> {
        let fuse_connection = self.fuse_connection.take().unwrap();
        let fs = self.filesystem.take().expect("filesystem not init");
        // defer worker initialization until after FUSE INIT handshake

        let max_write = self.init_filesystem(&fs, &fuse_connection).await?.get() as usize;
        let workers_active = self.worker_count > 1;
        if workers_active {
            self.ensure_workers(fs.clone());
        }
        let buffer_size = (max_write + FUSE_WRITE_IN_SIZE).max(FUSE_MIN_READ_BUFFER_SIZE);

        let mut header_buffer = vec![0; FUSE_IN_HEADER_SIZE];
        let mut data_buffer = vec![0; buffer_size];

        loop {
            if workers_active {
                while self.inflight.load(Ordering::Acquire) >= self.max_background {
                    self.inflight_notify.notified().await;
                }
            }
            let in_header = match self
                .read_fuse_request(&fuse_connection, header_buffer, data_buffer)
                .await
            {
                ReadResult::Destroy => {
                    fs.destroy(Request {
                        unique: 0,
                        uid: 0,
                        gid: 0,
                        pid: 0,
                    })
                    .await;

                    return Ok(());
                }

                ReadResult::Request {
                    in_header,
                    header_buffer: header_buf,
                    data_buffer: data_buf,
                } => {
                    header_buffer = header_buf;
                    data_buffer = data_buf;

                    match in_header {
                        Err(_) => continue,

                        Ok(in_header) => in_header,
                    }
                }
            };

            let request = Request::from(&in_header);

            let opcode = match fuse_opcode::try_from(in_header.opcode) {
                Err(err) => {
                    debug!("receive unknown opcode {}", err.0);

                    reply_error_in_place(libc::ENOSYS.into(), request, &self.response_sender).await;

                    continue;
                }

                Ok(opcode) => opcode,
            };

            debug!("receive opcode {}", opcode);

            let data_size = in_header.len as usize - FUSE_IN_HEADER_SIZE;
            let data_ref = &data_buffer[..data_size];

            if let Some(workers) = &self.workers {
                let unique = request.unique;
                let opcode_raw = in_header.opcode;
                let body_vec = data_ref.to_vec();
                let lite = InHeaderLite {
                    nodeid: in_header.nodeid,
                    uid: in_header.uid,
                    gid: in_header.gid,
                    pid: in_header.pid,
                };
                let _inflight_guard =
                    InflightGuard::new(self.inflight.clone(), self.inflight_notify.clone());
                workers
                    .submit(WorkItem {
                        unique,
                        opcode: opcode_raw,
                        in_header: lite,
                        data: body_vec,
                        _inflight_guard,
                    })
                    .await;
            } else {
                // Will concurrency in a single-threaded context cause disorder in the sequence of operations on a single file?
                match opcode {
                    fuse_opcode::FUSE_INIT => {
                        warn!("duplicated fuse init request");

                        self.handle_init(request, data_ref, &fuse_connection, &fs)
                            .await?;
                    }

                    fuse_opcode::FUSE_DESTROY => {
                        debug!("receive fuse destroy");

                        fs.destroy(request).await;

                        debug!("fuse destroyed");

                        return Ok(());
                    }

                    fuse_opcode::FUSE_LOOKUP => {
                        if !workers_active {
                            self.handle_lookup(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_FORGET => {
                        self.handle_forget(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_GETATTR => {
                        if !workers_active {
                            self.handle_getattr(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_SETATTR => {
                        self.handle_setattr(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_READLINK => {
                        self.handle_readlink(request, in_header, &fs).await;
                    }

                    fuse_opcode::FUSE_SYMLINK => {
                        self.handle_symlink(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_MKNOD => {
                        self.handle_mknod(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_MKDIR => {
                        self.handle_mkdir(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_UNLINK => {
                        self.handle_unlink(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_RMDIR => {
                        self.handle_rmdir(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_RENAME => {
                        self.handle_rename(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_LINK => {
                        self.handle_link(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_OPEN => {
                        if !workers_active {
                            self.handle_open(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_READ => {
                        if !workers_active {
                            self.handle_read(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_WRITE => {
                        if !workers_active {
                            self.handle_write(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_STATFS => {
                        self.handle_statfs(request, in_header, &fs).await;
                    }

                    fuse_opcode::FUSE_RELEASE => {
                        self.handle_release(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_FSYNC => {
                        self.handle_fsync(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_SETXATTR => {
                        self.handle_setxattr(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_GETXATTR => {
                        self.handle_getxattr(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_LISTXATTR => {
                        self.handle_listxattr(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_REMOVEXATTR => {
                        self.handle_removexattr(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_FLUSH => {
                        self.handle_flush(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_OPENDIR => {
                        self.handle_opendir(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_READDIR => {
                        if !workers_active {
                            self.handle_readdir(request, in_header, data_ref, &fs).await;
                        }
                    }

                    fuse_opcode::FUSE_RELEASEDIR => {
                        self.handle_releasedir(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_FSYNCDIR => {
                        self.handle_fsyncdir(request, in_header, data_ref, &fs)
                            .await;
                    }

                    #[cfg(feature = "file-lock")]
                    fuse_opcode::FUSE_GETLK => {
                        self.handle_getlk(request, in_header, data_ref, &fs).await;
                    }

                    #[cfg(feature = "file-lock")]
                    fuse_opcode::FUSE_SETLK | fuse_opcode::FUSE_SETLKW => {
                        self.handle_setlk(
                            request,
                            in_header,
                            data_ref,
                            opcode == fuse_opcode::FUSE_SETLKW,
                            &fs,
                        )
                        .await;
                    }

                    fuse_opcode::FUSE_ACCESS => {
                        self.handle_access(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_CREATE => {
                        self.handle_create(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_INTERRUPT => {
                        self.handle_interrupt(request, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_BMAP => {
                        self.handle_bmap(request, in_header, data_ref, &fs).await;
                    }

                    /*fuse_opcode::FUSE_IOCTL => {
                        let mut resp_sender = self.response_sender.clone();

                        let ioctl_in = match get_bincode_config().deserialize::<fuse_ioctl_in>(data) {
                            Err(err) => {
                                error!("deserialize fuse_ioctl_in failed {}", err);

                                 reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                                continue;
                            }

                            Ok(ioctl_in) => ioctl_in,
                        };

                        let ioctl_data = (&data[FUSE_IOCTL_IN_SIZE..]).to_vec();

                        let fs = fs.clone();
                    }*/
                    fuse_opcode::FUSE_POLL => {
                        self.handle_poll(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_NOTIFY_REPLY => {
                        self.handle_notify_reply(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_BATCH_FORGET => {
                        self.handle_batch_forget(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_FALLOCATE => {
                        self.handle_fallocate(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_READDIRPLUS => {
                        self.handle_readdirplus(request, in_header, data_ref, &fs)
                            .await;
                    }

                    fuse_opcode::FUSE_RENAME2 => {
                        self.handle_rename2(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_LSEEK => {
                        self.handle_lseek(request, in_header, data_ref, &fs).await;
                    }

                    fuse_opcode::FUSE_COPY_FILE_RANGE => {
                        self.handle_copy_file_range(request, in_header, data_ref, &fs)
                            .await;
                    }

                    #[cfg(target_os = "macos")]
                    fuse_opcode::FUSE_SETVOLNAME => {}

                    #[cfg(target_os = "macos")]
                    fuse_opcode::FUSE_GETXTIMES => {}

                    #[cfg(target_os = "macos")]
                    fuse_opcode::FUSE_EXCHANGE => {} // fuse_opcode::CUSE_INIT => {}
                }
            }
        }
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_init(
        &mut self,
        request: Request,
        data: &[u8],
        fuse_connection: &FuseConnection,
        fs: &FS,
    ) -> IoResult<NonZeroU32> {
        let init_in = match get_bincode_config().deserialize::<fuse_init_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_init_in failed {}, request unique {}",
                    err, request.unique
                );

                let init_out_header = fuse_out_header {
                    len: FUSE_OUT_HEADER_SIZE as u32,
                    error: libc::EINVAL,
                    unique: request.unique,
                };

                let init_out_header_data = get_bincode_config()
                    .serialize(&init_out_header)
                    .expect("won't happened");

                if let Err(err) = fuse_connection
                    .write_vectored::<_, Vec<u8>>(init_out_header_data, None)
                    .await
                    .1
                {
                    error!("write error init out data to /dev/fuse failed {}", err);
                }

                return Err(IoError::from_raw_os_error(libc::EINVAL));
            }

            Ok(init_in) => init_in,
        };

        debug!("fuse_init {:?}", init_in);

        let mut reply_flags = 0;

        // TODO: most of these FUSE_* flags should be controllable by the consuming crate.
        if init_in.flags & FUSE_ASYNC_READ > 0 {
            debug!("enable FUSE_ASYNC_READ");

            reply_flags |= FUSE_ASYNC_READ;
        }

        #[cfg(feature = "file-lock")]
        if init_in.flags & FUSE_POSIX_LOCKS > 0 {
            debug!("enable FUSE_POSIX_LOCKS");

            reply_flags |= FUSE_POSIX_LOCKS;
        }

        if init_in.flags & FUSE_FILE_OPS > 0 {
            debug!("enable FUSE_FILE_OPS");

            reply_flags |= FUSE_FILE_OPS;
        }

        if init_in.flags & FUSE_ATOMIC_O_TRUNC > 0 {
            debug!("enable FUSE_ATOMIC_O_TRUNC");

            reply_flags |= FUSE_ATOMIC_O_TRUNC;
        }

        if init_in.flags & FUSE_EXPORT_SUPPORT > 0 {
            debug!("enable FUSE_EXPORT_SUPPORT");

            reply_flags |= FUSE_EXPORT_SUPPORT;
        }

        if init_in.flags & FUSE_BIG_WRITES > 0 {
            debug!("enable FUSE_BIG_WRITES");

            reply_flags |= FUSE_BIG_WRITES;
        }

        if init_in.flags & FUSE_DONT_MASK > 0 && self.mount_options.dont_mask {
            debug!("enable FUSE_DONT_MASK");

            reply_flags |= FUSE_DONT_MASK;
        }

        #[cfg(not(target_os = "macos"))]
        if init_in.flags & FUSE_SPLICE_WRITE > 0 {
            debug!("enable FUSE_SPLICE_WRITE");

            reply_flags |= FUSE_SPLICE_WRITE;
        }

        #[cfg(not(target_os = "macos"))]
        if init_in.flags & FUSE_SPLICE_MOVE > 0 {
            debug!("enable FUSE_SPLICE_MOVE");

            reply_flags |= FUSE_SPLICE_MOVE;
        }

        #[cfg(not(target_os = "macos"))]
        if init_in.flags & FUSE_SPLICE_READ > 0 {
            debug!("enable FUSE_SPLICE_READ");

            reply_flags |= FUSE_SPLICE_READ;
        }

        // posix lock used, maybe we don't need bsd lock
        /*if init_in.flags&FUSE_FLOCK_LOCKS>0 {
            reply_flags |= FUSE_FLOCK_LOCKS;
        }*/

        /*if init_in.flags & FUSE_HAS_IOCTL_DIR > 0 {
            debug!("enable FUSE_HAS_IOCTL_DIR");

            reply_flags |= FUSE_HAS_IOCTL_DIR;
        }*/

        if init_in.flags & FUSE_AUTO_INVAL_DATA > 0 {
            debug!("enable FUSE_AUTO_INVAL_DATA");

            reply_flags |= FUSE_AUTO_INVAL_DATA;
        }

        if init_in.flags & FUSE_DO_READDIRPLUS > 0 || self.mount_options.force_readdir_plus {
            debug!("enable FUSE_DO_READDIRPLUS");

            reply_flags |= FUSE_DO_READDIRPLUS;
        }

        if init_in.flags & FUSE_READDIRPLUS_AUTO > 0 && !self.mount_options.force_readdir_plus {
            debug!("enable FUSE_READDIRPLUS_AUTO");

            reply_flags |= FUSE_READDIRPLUS_AUTO;
        }

        if init_in.flags & FUSE_ASYNC_DIO > 0 {
            debug!("enable FUSE_ASYNC_DIO");

            reply_flags |= FUSE_ASYNC_DIO;
        }

        if init_in.flags & FUSE_WRITEBACK_CACHE > 0 && self.mount_options.write_back {
            debug!("enable FUSE_WRITEBACK_CACHE");

            reply_flags |= FUSE_WRITEBACK_CACHE;
        }

        if init_in.flags & FUSE_NO_OPEN_SUPPORT > 0 && self.mount_options.no_open_support {
            debug!("enable FUSE_NO_OPEN_SUPPORT");

            reply_flags |= FUSE_NO_OPEN_SUPPORT;
        }

        if init_in.flags & FUSE_PARALLEL_DIROPS > 0 {
            debug!("enable FUSE_PARALLEL_DIROPS");

            reply_flags |= FUSE_PARALLEL_DIROPS;
        }

        if init_in.flags & FUSE_HANDLE_KILLPRIV > 0 && self.mount_options.handle_killpriv {
            debug!("enable FUSE_HANDLE_KILLPRIV");

            reply_flags |= FUSE_HANDLE_KILLPRIV;
        }

        if init_in.flags & FUSE_POSIX_ACL > 0 && self.mount_options.default_permissions {
            debug!("enable FUSE_POSIX_ACL");

            reply_flags |= FUSE_POSIX_ACL;
        }

        if init_in.flags & FUSE_MAX_PAGES > 0 {
            debug!("enable FUSE_MAX_PAGES");

            reply_flags |= FUSE_MAX_PAGES;
        }

        if init_in.flags & FUSE_CACHE_SYMLINKS > 0 {
            debug!("enable FUSE_CACHE_SYMLINKS");

            reply_flags |= FUSE_CACHE_SYMLINKS;
        }

        if init_in.flags & FUSE_NO_OPENDIR_SUPPORT > 0 && self.mount_options.no_open_dir_support {
            debug!("enable FUSE_NO_OPENDIR_SUPPORT");

            reply_flags |= FUSE_NO_OPENDIR_SUPPORT;
        }

        #[cfg(target_os = "macos")]
        if init_in.flags & FUSE_ALLOCATE > 0 {
            debug!("enable FUSE_ALLOCATE");

            reply_flags |= FUSE_ALLOCATE;
        }

        #[cfg(target_os = "macos")]
        if init_in.flags & FUSE_EXCHANGE_DATA > 0 {
            debug!("enable FUSE_EXCHANGE_DATA");

            reply_flags |= FUSE_EXCHANGE_DATA;
        }

        #[cfg(target_os = "macos")]
        if init_in.flags & FUSE_CASE_INSENSITIVE > 0 {
            debug!("enable FUSE_CASE_INSENSITIVE");

            reply_flags |= FUSE_CASE_INSENSITIVE;
        }

        #[cfg(target_os = "macos")]
        if init_in.flags & FUSE_VOL_RENAME > 0 {
            debug!("enable FUSE_VOL_RENAME");

            reply_flags |= FUSE_VOL_RENAME;
        }

        #[cfg(target_os = "macos")]
        if init_in.flags & FUSE_XTIMES > 0 {
            debug!("enable FUSE_XTIMES");

            reply_flags |= FUSE_XTIMES;
        }

        // TODO: pass init_in to init, so the file system will know which flags are in use.
        let reply = match fs.init(request).await {
            Err(err) => {
                let init_out_header = fuse_out_header {
                    len: FUSE_OUT_HEADER_SIZE as u32,
                    error: err.into(),
                    unique: request.unique,
                };

                let init_out_header_data = get_bincode_config()
                    .serialize(&init_out_header)
                    .expect("won't happened");

                if let Err(err) = fuse_connection
                    .write_vectored::<_, Vec<u8>>(init_out_header_data, None)
                    .await
                    .1
                {
                    error!("write error init out data to /dev/fuse failed {}", err);
                }

                return Err(err.into());
            }

            Ok(reply) => reply,
        };

        let init_out = fuse_init_out {
            major: FUSE_KERNEL_VERSION,
            minor: FUSE_KERNEL_MINOR_VERSION,
            max_readahead: init_in.max_readahead,
            flags: reply_flags,
            max_background: DEFAULT_MAX_BACKGROUND,
            congestion_threshold: DEFAULT_CONGESTION_THRESHOLD,
            max_write: reply.max_write.get(),
            time_gran: DEFAULT_TIME_GRAN,
            max_pages: DEFAULT_MAX_PAGES,
            map_alignment: DEFAULT_MAP_ALIGNMENT,
            unused: [0; 8],
        };

        debug!("fuse init out {:?}", init_out);

        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE + FUSE_INIT_OUT_SIZE) as u32,
            error: 0,
            unique: request.unique,
        };

        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_INIT_OUT_SIZE);

        get_bincode_config()
            .serialize_into(&mut data, &out_header)
            .expect("won't happened");
        get_bincode_config()
            .serialize_into(&mut data, &init_out)
            .expect("won't happened");

        if let Err(err) = fuse_connection
            .write_vectored::<_, Vec<u8>>(data, None)
            .await
            .1
        {
            error!("write init out data to /dev/fuse failed {}", err);

            return Err(err);
        }

        debug!("fuse init done");

        Ok(reply.max_write)
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_lookup(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let name = match get_first_null_position(data) {
            None => {
                error!("lookup body has no null, request unique {}", request.unique);

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_lookup"), async move {
            debug!(
                "lookup unique {} name {:?} in parent {}",
                request.unique, name, in_header.nodeid
            );

            let data = match fs.lookup(request, in_header.nodeid, &name).await {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: request.unique,
                    };

                    get_bincode_config()
                        .serialize(&out_header)
                        .expect("won't happened")
                }

                Ok(entry) => {
                    let entry_out: fuse_entry_out = entry.into();

                    debug!("lookup response {:?}", entry_out);

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &entry_out)
                        .expect("won't happened");

                    data
                }
            };

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    /// if Ok(true), quit the dispatch
    #[instrument(skip(self, data, fs))]
    async fn handle_forget(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let forget_in = match get_bincode_config().deserialize::<fuse_forget_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_forget_in failed {}, request unique {}",
                    err, request.unique
                );

                // no need to reply
                return;
            }

            Ok(forget_in) => forget_in,
        };

        let fs = fs.clone();

        spawn(debug_span!("fuse_forget"), async move {
            debug!(
                "forget unique {} inode {} nlookup {}",
                request.unique, in_header.nodeid, forget_in.nlookup
            );

            fs.forget(request, in_header.nodeid, forget_in.nlookup)
                .await
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_getattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let getattr_in = match get_bincode_config().deserialize::<fuse_getattr_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_forget_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(getattr_in) => getattr_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_getattr"), async move {
            debug!(
                "getattr unique {} inode {}",
                request.unique, in_header.nodeid
            );

            let fh = if getattr_in.getattr_flags & FUSE_GETATTR_FH > 0 {
                Some(getattr_in.fh)
            } else {
                None
            };

            let data = match fs
                .getattr(request, in_header.nodeid, fh, getattr_in.getattr_flags)
                .await
            {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: request.unique,
                    };

                    get_bincode_config()
                        .serialize(&out_header)
                        .expect("won't happened")
                }

                Ok(attr) => {
                    let attr_out = fuse_attr_out {
                        attr_valid: attr.ttl.as_secs(),
                        attr_valid_nsec: attr.ttl.subsec_nanos(),
                        dummy: getattr_in.dummy,
                        attr: attr.attr.into(),
                    };

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &attr_out)
                        .expect("won't happened");

                    data
                }
            };

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_setattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let setattr_in = match get_bincode_config().deserialize::<fuse_setattr_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_setattr_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(setattr_in) => setattr_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_setattr"), async move {
            let set_attr = SetAttr::from(&setattr_in);

            let fh = if setattr_in.valid & FATTR_FH > 0 {
                Some(setattr_in.fh)
            } else {
                None
            };

            debug!(
                "setattr unique {} inode {} set_attr {:?}",
                request.unique, in_header.nodeid, set_attr
            );

            let data = match fs.setattr(request, in_header.nodeid, fh, set_attr).await {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: request.unique,
                    };

                    get_bincode_config()
                        .serialize(&out_header)
                        .expect("won't happened")
                }

                Ok(attr) => {
                    let attr_out: fuse_attr_out = attr.into();

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &attr_out)
                        .expect("won't happened");

                    data
                }
            };

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, fs))]
    async fn handle_readlink(&mut self, request: Request, in_header: fuse_in_header, fs: &Arc<FS>) {
        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_readlink"), async move {
            debug!(
                "readlink unique {} inode {}",
                request.unique, in_header.nodeid
            );

            let data = match fs.readlink(request, in_header.nodeid).await {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: request.unique,
                    };

                    Either::Left(
                        get_bincode_config()
                            .serialize(&out_header)
                            .expect("won't happened"),
                    )
                }

                Ok(data) => {
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + data.data.len()) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data_buf, &out_header)
                        .expect("won't happened");

                    Either::Right((data_buf, data.data))
                }
            };

            let _ = resp_sender.send(data).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_symlink(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let (name, first_null_index) = match get_first_null_position(data) {
            None => {
                error!("symlink has no null, request unique {}", request.unique);

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
        };

        data = &data[first_null_index + 1..];

        let link_name = match get_first_null_position(data) {
            None => {
                error!(
                    "symlink has no second null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_symlink"), async move {
            debug!(
                "symlink unique {} parent {} name {:?} link {:?}",
                request.unique, in_header.nodeid, name, link_name
            );

            let data = match fs
                .symlink(request, in_header.nodeid, &name, &link_name)
                .await
            {
                Err(err) => {
                    let out_header = fuse_out_header {
                        len: FUSE_OUT_HEADER_SIZE as u32,
                        error: err.into(),
                        unique: request.unique,
                    };

                    get_bincode_config()
                        .serialize(&out_header)
                        .expect("won't happened")
                }

                Ok(entry) => {
                    let entry_out: fuse_entry_out = entry.into();

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &entry_out)
                        .expect("won't happened");

                    data
                }
            };

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_mknod(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let mknod_in = match get_bincode_config().deserialize::<fuse_mknod_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_mknod_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(mknod_in) => mknod_in,
        };

        data = &data[FUSE_MKNOD_IN_SIZE..];

        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_mknod_in body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_mknod"), async move {
            debug!(
                "mknod unique {} parent {} name {:?} {:?}",
                request.unique, in_header.nodeid, name, mknod_in
            );

            match fs
                .mknod(
                    request,
                    in_header.nodeid,
                    &name,
                    mknod_in.mode,
                    mknod_in.rdev,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;
                }

                Ok(entry) => {
                    let entry_out: fuse_entry_out = entry.into();

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &entry_out)
                        .expect("won't happened");

                    let _ = resp_sender.send(Either::Left(data)).await;
                }
            }
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_mkdir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let mkdir_in = match get_bincode_config().deserialize::<fuse_mkdir_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_mknod_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(mkdir_in) => mkdir_in,
        };

        data = &data[FUSE_MKDIR_IN_SIZE..];

        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "deserialize fuse_mknod_in doesn't have null unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_mkdir"), async move {
            debug!(
                "mkdir unique {} parent {} name {:?} {:?}",
                request.unique, in_header.nodeid, name, mkdir_in
            );

            match fs
                .mkdir(
                    request,
                    in_header.nodeid,
                    &name,
                    mkdir_in.mode,
                    mkdir_in.umask,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;
                }

                Ok(entry) => {
                    let entry_out: fuse_entry_out = entry.into();

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &entry_out)
                        .expect("won't happened");

                    let _ = resp_sender.send(Either::Left(data)).await;
                }
            }
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_unlink(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "unlink body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_unlink"), async move {
            debug!(
                "unlink unique {} parent {} name {:?}",
                request.unique, in_header.nodeid, name
            );

            let resp_value = if let Err(err) = fs.unlink(request, in_header.nodeid, &name).await {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_rmdir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "rmdir body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_rmdir"), async move {
            debug!(
                "rmdir unique {} parent {} name {:?}",
                request.unique, in_header.nodeid, name
            );

            let resp_value = if let Err(err) = fs.rmdir(request, in_header.nodeid, &name).await {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_rename(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let rename_in = match get_bincode_config().deserialize::<fuse_rename_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_rename_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(rename_in) => rename_in,
        };

        data = &data[FUSE_RENAME_IN_SIZE..];

        let (name, first_null_index) = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_rename_in body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
        };

        data = &data[first_null_index + 1..];

        let new_name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_rename_in body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_rename"), async move {
            debug!(
                "rename unique {} parent {} name {:?} new parent {} new name {:?}",
                request.unique, in_header.nodeid, name, rename_in.newdir, new_name
            );

            let resp_value = if let Err(err) = fs
                .rename(
                    request,
                    in_header.nodeid,
                    &name,
                    rename_in.newdir,
                    &new_name,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_link(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let link_in = match get_bincode_config().deserialize::<fuse_link_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_link_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(link_in) => link_in,
        };

        data = &data[FUSE_LINK_IN_SIZE..];

        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_link_in body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_link"), async move {
            debug!(
                "link unique {} inode {} new parent {} new name {:?}",
                request.unique, link_in.oldnodeid, in_header.nodeid, name
            );

            match fs
                .link(request, link_in.oldnodeid, in_header.nodeid, &name)
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;
                }

                Ok(entry) => {
                    let entry_out: fuse_entry_out = entry.into();

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &entry_out)
                        .expect("won't happened");

                    let _ = resp_sender.send(Either::Left(data)).await;
                }
            }
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_open(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let open_in = match get_bincode_config().deserialize::<fuse_open_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_open_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(open_in) => open_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_open"), async move {
            debug!(
                "open unique {} inode {} flags {}",
                request.unique, in_header.nodeid, open_in.flags
            );

            let opened = match fs.open(request, in_header.nodeid, open_in.flags).await {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(opened) => opened,
            };

            let open_out: fuse_open_out = opened.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &open_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_read(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let read_in = match get_bincode_config().deserialize::<fuse_read_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_read_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(read_in) => read_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_read"), async move {
            debug!(
                "read unique {} inode {} {:?}",
                request.unique, in_header.nodeid, read_in
            );

            let mut reply_data = match fs
                .read(
                    request,
                    in_header.nodeid,
                    read_in.fh,
                    read_in.offset,
                    read_in.size,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_data) => reply_data.data,
            };

            if reply_data.len() > read_in.size as _ {
                reply_data.truncate(read_in.size as _);
            }

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + reply_data.len()) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data_buf = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

            get_bincode_config()
                .serialize_into(&mut data_buf, &out_header)
                .expect("won't happened");

            let _ = resp_sender
                .send(Either::Right((data_buf, reply_data)))
                .await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_write(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let write_in = match get_bincode_config().deserialize::<fuse_write_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_write_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(write_in) => write_in,
        };

        data = &data[FUSE_WRITE_IN_SIZE..];

        if write_in.size as usize != data.len() {
            error!("fuse_write_in body len is invalid");

            reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

            return;
        }

        let data = data.to_vec();

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_write"), async move {
            debug!(
                "write unique {} inode {} {:?}",
                request.unique, in_header.nodeid, write_in
            );

            // Special handling for O_DIRECT writes: ensure the buffer is 4096-byte aligned.
            // Currently we only encounter this case in xfstests.
            use std::borrow::Cow;
            const ALIGNMENT: usize = 4096;
            let mut aligned_buf =
                aligned_box::AlignedBox::<[u8]>::slice_from_default(ALIGNMENT, data.len()).unwrap();
            aligned_buf.copy_from_slice(&data);
            let aligned_data: Cow<[u8]> = if data.as_ptr().align_offset(ALIGNMENT) == 0 {
                Cow::Borrowed(&data)
            } else {
                Cow::Borrowed(&aligned_buf)
            };

            let reply_write = match fs
                .write(
                    request,
                    in_header.nodeid,
                    write_in.fh,
                    write_in.offset,
                    &aligned_data,
                    write_in.write_flags,
                    write_in.flags,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_write) => reply_write,
            };

            let write_out: fuse_write_out = reply_write.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &write_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, fs))]
    async fn handle_statfs(&mut self, request: Request, in_header: fuse_in_header, fs: &Arc<FS>) {
        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_statfs"), async move {
            debug!(
                "statfs unique {} inode {}",
                request.unique, in_header.nodeid
            );

            let fs_stat = match fs.statfs(request, in_header.nodeid).await {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(fs_stat) => fs_stat,
            };

            let statfs_out: fuse_statfs_out = fs_stat.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &statfs_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_release(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let release_in = match get_bincode_config().deserialize::<fuse_release_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_release_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(release_in) => release_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_release"), async move {
            let flush = release_in.release_flags & FUSE_RELEASE_FLUSH > 0;

            debug!(
                "release unique {} inode {} fh {} flags {} lock_owner {} flush {}",
                request.unique,
                in_header.nodeid,
                release_in.fh,
                release_in.flags,
                release_in.lock_owner,
                flush
            );

            let resp_value = if let Err(err) = fs
                .release(
                    request,
                    in_header.nodeid,
                    release_in.fh,
                    release_in.flags,
                    release_in.lock_owner,
                    flush,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_fsync(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let fsync_in = match get_bincode_config().deserialize::<fuse_fsync_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_fsync_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(fsync_in) => fsync_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_fsync"), async move {
            let data_sync = fsync_in.fsync_flags & 1 > 0;

            debug!(
                "fsync unique {} inode {} fh {} data_sync {}",
                request.unique, in_header.nodeid, fsync_in.fh, data_sync
            );

            let resp_value = if let Err(err) = fs
                .fsync(request, in_header.nodeid, fsync_in.fh, data_sync)
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_setxattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let setxattr_in = match get_bincode_config().deserialize::<fuse_setxattr_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_setxattr_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(setxattr_in) => setxattr_in,
        };

        data = &data[FUSE_SETXATTR_IN_SIZE..];

        let (name, first_null_index) = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_setxattr_in body has no null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
        };

        data = &data[first_null_index + 1..];

        // setxattr "size" field specifies size of only "Value" part of data
        if setxattr_in.size as usize != data.len() {
            error!(
                "fuse_setxattr_in value field data length is not right, request unique {} setxattr_in.size={} data.len={}", request.unique, setxattr_in.size, data.len());

            reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

            return;
        }

        let data = data.to_vec();

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_setxattr"), async move {
            debug!(
                "setxattr unique {} inode {}",
                request.unique, in_header.nodeid
            );

            // TODO handle os X argument
            let resp_value = if let Err(err) = fs
                .setxattr(
                    request,
                    in_header.nodeid,
                    &name,
                    &data,
                    setxattr_in.flags,
                    0,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_getxattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let getxattr_in = match get_bincode_config().deserialize::<fuse_getxattr_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_getxattr_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(getxattr_in) => getxattr_in,
        };

        data = &data[FUSE_GETXATTR_IN_SIZE..];

        let name = match get_first_null_position(data) {
            None => {
                error!("fuse_getxattr_in body has no null {}", request.unique);

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_getxattr"), async move {
            debug!(
                "getxattr unique {} inode {}",
                request.unique, in_header.nodeid
            );

            let xattr = match fs
                .getxattr(request, in_header.nodeid, &name, getxattr_in.size)
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(xattr) => xattr,
            };

            let data = match xattr {
                ReplyXAttr::Size(size) => {
                    let getxattr_out = fuse_getxattr_out { size, _padding: 0 };

                    // Found in xfstests.
                    // FUSE getxattr follows a two-step protocol:
                    // 1) When size == 0, the kernel is only querying the required buffer size.
                    //    The filesystem should return success and provide the attribute length.
                    // 2) When size > 0, the kernel expects the actual attribute data.
                    //    If the provided buffer is too small (attribute grew), return ERANGE
                    //    and report the new required size.
                    let error_code = if getxattr_in.size == 0 {
                        0
                    } else {
                        libc::ERANGE
                    };
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                        error: error_code,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &getxattr_out)
                        .expect("won't happened");

                    Either::Left(data)
                }

                ReplyXAttr::Data(xattr_data) => {
                    // TODO check is right way or not
                    // TODO should we check data length or not
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + xattr_data.len()) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");

                    Either::Right((data, xattr_data))
                }
            };

            let _ = resp_sender.send(data).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_listxattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let listxattr_in = match get_bincode_config().deserialize::<fuse_getxattr_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_getxattr_in in listxattr failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(listxattr_in) => listxattr_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_listxattr"), async move {
            debug!(
                "listxattr unique {} inode {} size {}",
                request.unique, in_header.nodeid, listxattr_in.size
            );

            let xattr = match fs
                .listxattr(request, in_header.nodeid, listxattr_in.size)
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(xattr) => xattr,
            };

            let data = match xattr {
                ReplyXAttr::Size(size) => {
                    let getxattr_out = fuse_getxattr_out { size, _padding: 0 };

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    get_bincode_config()
                        .serialize_into(&mut data, &getxattr_out)
                        .expect("won't happened");

                    Either::Left(data)
                }

                ReplyXAttr::Data(xattr_data) => {
                    // TODO check is right way or not
                    // TODO should we check data length or not
                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + xattr_data.len()) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

                    get_bincode_config()
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");

                    Either::Right((data, xattr_data))
                }
            };

            let _ = resp_sender.send(data).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_removexattr(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse removexattr body has no null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_removexattr"), async move {
            debug!(
                "removexattr unique {} inode {}",
                request.unique, in_header.nodeid
            );

            let resp_value =
                if let Err(err) = fs.removexattr(request, in_header.nodeid, &name).await {
                    err.into()
                } else {
                    0
                };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_flush(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let flush_in = match get_bincode_config().deserialize::<fuse_flush_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_flush_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(flush_in) => flush_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_flush"), async move {
            debug!(
                "flush unique {} inode {} fh {} lock_owner {}",
                request.unique, in_header.nodeid, flush_in.fh, flush_in.lock_owner
            );

            let resp_value = if let Err(err) = fs
                .flush(request, in_header.nodeid, flush_in.fh, flush_in.lock_owner)
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_opendir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let open_in = match get_bincode_config().deserialize::<fuse_open_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_open_in in opendir failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(open_in) => open_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_opendir"), async move {
            debug!(
                "opendir unique {} inode {} flags {}",
                request.unique, in_header.nodeid, open_in.flags
            );

            let reply_open = match fs.opendir(request, in_header.nodeid, open_in.flags).await {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_open) => reply_open,
            };

            let open_out: fuse_open_out = reply_open.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &open_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_readdir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        if self.mount_options.force_readdir_plus {
            reply_error_in_place(libc::ENOSYS.into(), request, &self.response_sender).await;

            return;
        }

        let read_in = match get_bincode_config().deserialize::<fuse_read_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_read_in in readdir failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(read_in) => read_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_readdir"), async move {
            debug!(
                "readdir unique {} inode {} fh {} offset {}",
                request.unique, in_header.nodeid, read_in.fh, read_in.offset
            );

            let reply_readdir = match fs
                .readdir(request, in_header.nodeid, read_in.fh, read_in.offset as i64)
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_readdir) => reply_readdir,
            };

            let max_size = read_in.size as usize;

            let mut entry_data = Vec::with_capacity(max_size);

            let entries = reply_readdir.entries;
            let mut entries = pin!(entries);

            while let Some(entry) = entries.next().await {
                let entry = match entry {
                    Err(err) => {
                        reply_error_in_place(err, request, resp_sender).await;

                        return;
                    }

                    Ok(entry) => entry,
                };

                let name = &entry.name;

                let dir_entry_size = FUSE_DIRENT_SIZE + name.len();

                let padding_size = get_padding_size(dir_entry_size);

                if entry_data.len() + dir_entry_size > max_size {
                    break;
                }

                let dir_entry = fuse_dirent {
                    ino: entry.inode,
                    off: entry.offset as u64,
                    namelen: name.len() as u32,
                    // learn from fuse-rs and golang bazil.org fuse DirentType
                    r#type: mode_from_kind_and_perm(entry.kind, 0) >> 12,
                };

                get_bincode_config()
                    .serialize_into(&mut entry_data, &dir_entry)
                    .expect("won't happened");

                entry_data.extend_from_slice(name.as_bytes());

                // padding
                entry_data.resize(entry_data.len() + padding_size, 0);
            }

            // TODO find a way to avoid multi allocate

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");

            let _ = resp_sender
                .send(Either::Right((data, entry_data.into())))
                .await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_releasedir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let release_in = match get_bincode_config().deserialize::<fuse_release_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_release_in in releasedir failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(release_in) => release_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_releasedir"), async move {
            debug!(
                "releasedir unique {} inode {} fh {} flags {}",
                request.unique, in_header.nodeid, release_in.fh, release_in.flags
            );

            let resp_value = if let Err(err) = fs
                .releasedir(request, in_header.nodeid, release_in.fh, release_in.flags)
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_fsyncdir(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let fsync_in = match get_bincode_config().deserialize::<fuse_fsync_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_fsync_in in fsyncdir failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(fsync_in) => fsync_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_fsyncdir"), async move {
            let data_sync = fsync_in.fsync_flags & 1 > 0;

            debug!(
                "fsyncdir unique {} inode {} fh {} data_sync {}",
                request.unique, in_header.nodeid, fsync_in.fh, data_sync
            );

            let resp_value = if let Err(err) = fs
                .fsyncdir(request, in_header.nodeid, fsync_in.fh, data_sync)
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[cfg(feature = "file-lock")]
    #[instrument(skip(self, data, fs))]
    async fn handle_getlk(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let getlk_in = match get_bincode_config().deserialize::<fuse_lk_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_lk_in in getlk failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(getlk_in) => getlk_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_getlk"), async move {
            debug!(
                "getlk unique {} inode {} {:?}",
                request.unique, in_header.nodeid, getlk_in
            );

            let reply_lock = match fs
                .getlk(
                    request,
                    in_header.nodeid,
                    getlk_in.fh,
                    getlk_in.owner,
                    getlk_in.lk.start,
                    getlk_in.lk.end,
                    getlk_in.lk.r#type,
                    getlk_in.lk.pid,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_lock) => reply_lock,
            };

            let getlk_out: fuse_lk_out = reply_lock.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &getlk_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[cfg(feature = "file-lock")]
    #[instrument(skip(self, data, fs))]
    async fn handle_setlk(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        block: bool,
        fs: &Arc<FS>,
    ) {
        let setlk_in = match get_bincode_config().deserialize::<fuse_lk_in>(data) {
            Err(err) => {
                let opcode = if block {
                    fuse_opcode::FUSE_SETLKW
                } else {
                    fuse_opcode::FUSE_SETLK
                };

                error!(
                    "deserialize fuse_lk_in in {:?} failed {}, request unique {}",
                    opcode, err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(setlk_in) => setlk_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_setlk"), async move {
            debug!(
                "setlk unique {} inode {} block {} {:?}",
                request.unique, in_header.nodeid, block, setlk_in
            );

            let resp = if let Err(err) = fs
                .setlk(
                    request,
                    in_header.nodeid,
                    setlk_in.fh,
                    setlk_in.owner,
                    setlk_in.lk.start,
                    setlk_in.lk.end,
                    setlk_in.lk.r#type,
                    setlk_in.lk.pid,
                    block,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("can't serialize into vec");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_access(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let access_in = match get_bincode_config().deserialize::<fuse_access_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_access_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(access_in) => access_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_access"), async move {
            debug!(
                "access unique {} inode {} mask {}",
                request.unique, in_header.nodeid, access_in.mask
            );

            let resp_value =
                if let Err(err) = fs.access(request, in_header.nodeid, access_in.mask).await {
                    err.into()
                } else {
                    0
                };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            debug!("access response {}", resp_value);

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_create(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let create_in = match get_bincode_config().deserialize::<fuse_create_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_create_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(create_in) => create_in,
        };

        data = &data[FUSE_CREATE_IN_SIZE..];

        let name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_create_in body has no null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_create"), async move {
            debug!(
                "create unique {} parent {} name {:?} mode {} flags {}",
                request.unique, in_header.nodeid, name, create_in.mode, create_in.flags
            );

            let created = match fs
                .create(
                    request,
                    in_header.nodeid,
                    &name,
                    create_in.mode,
                    create_in.flags,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(created) => created,
            };

            let (entry_out, open_out): (fuse_entry_out, fuse_open_out) = created.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data =
                Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &entry_out)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &open_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_interrupt(&mut self, request: Request, data: &[u8], fs: &Arc<FS>) {
        let interrupt_in = match get_bincode_config().deserialize::<fuse_interrupt_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_interrupt_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(interrupt_in) => interrupt_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_interrupt"), async move {
            debug!(
                "interrupt_in unique {} interrupt unique {}",
                request.unique, interrupt_in.unique
            );

            let resp_value = if let Err(err) = fs.interrupt(request, interrupt_in.unique).await {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_bmap(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let bmap_in = match get_bincode_config().deserialize::<fuse_bmap_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_bmap_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(bmap_in) => bmap_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_bmap"), async move {
            debug!(
                "bmap unique {} inode {} block size {} idx {}",
                request.unique, in_header.nodeid, bmap_in.blocksize, bmap_in.block
            );

            let reply_bmap = match fs
                .bmap(request, in_header.nodeid, bmap_in.blocksize, bmap_in.block)
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_bmap) => reply_bmap,
            };

            let bmap_out: fuse_bmap_out = reply_bmap.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &bmap_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_poll(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let poll_in = match get_bincode_config().deserialize::<fuse_poll_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_poll_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(poll_in) => poll_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        let notify = self.get_notify();

        spawn(debug_span!("fuse_poll"), async move {
            debug!(
                "poll unique {} inode {} {:?}",
                request.unique, in_header.nodeid, poll_in
            );

            let kh = if poll_in.flags & FUSE_POLL_SCHEDULE_NOTIFY > 0 {
                Some(poll_in.kh)
            } else {
                None
            };

            let reply_poll = match fs
                .poll(
                    request,
                    in_header.nodeid,
                    poll_in.fh,
                    kh,
                    poll_in.flags,
                    poll_in.events,
                    &notify,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_poll) => reply_poll,
            };

            let poll_out: fuse_poll_out = reply_poll.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &poll_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_notify_reply(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let resp_sender = self.response_sender.clone();

        let notify_retrieve_in =
            match get_bincode_config().deserialize::<fuse_notify_retrieve_in>(data) {
                Err(err) => {
                    error!(
                        "deserialize fuse_notify_retrieve_in failed {}, request unique {}",
                        err, request.unique
                    );

                    // TODO need to reply or not?
                    return;
                }

                Ok(notify_retrieve_in) => notify_retrieve_in,
            };

        data = &data[FUSE_NOTIFY_RETRIEVE_IN_SIZE..];

        if data.len() < notify_retrieve_in.size as usize {
            error!(
                "fuse_notify_retrieve unique {} data size is not right",
                request.unique
            );

            // TODO need to reply or not?
            return;
        }

        let data = data[..notify_retrieve_in.size as usize].to_vec();

        let fs = fs.clone();

        spawn(debug_span!("fuse_notify_reply"), async move {
            if let Err(err) = fs
                .notify_reply(
                    request,
                    in_header.nodeid,
                    notify_retrieve_in.offset,
                    data.into(),
                )
                .await
            {
                reply_error_in_place(err, request, resp_sender).await;
            }
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_batch_forget(
        &mut self,
        request: Request,
        _in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let batch_forget_in = match get_bincode_config().deserialize::<fuse_batch_forget_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_batch_forget_in failed {}, request unique {}",
                    err, request.unique
                );

                // no need to reply
                return;
            }

            Ok(batch_forget_in) => batch_forget_in,
        };

        let mut forgets = vec![];

        data = &data[FUSE_BATCH_FORGET_IN_SIZE..];

        // TODO if has less data, should I return error?
        while data.len() >= FUSE_FORGET_ONE_SIZE {
            match get_bincode_config().deserialize::<fuse_forget_one>(data) {
                Err(err) => {
                    error!("deserialize fuse_batch_forget_in body fuse_forget_one failed {}, request unique {}", err, request.unique);

                    // no need to reply
                    return;
                }

                Ok(forget_one) => {
                    data = &data[FUSE_FORGET_ONE_SIZE..];

                    forgets.push(forget_one);
                }
            }
        }

        if forgets.len() != batch_forget_in.count as usize {
            error!(
                "fuse_forget_one count != fuse_batch_forget_in.count, request unique {}",
                request.unique
            );

            return;
        }

        let fs = fs.clone();

        spawn(debug_span!("fuse_batch_forget"), async move {
            let inodes = forgets
                .into_iter()
                .map(|forget_one| (forget_one.nodeid, forget_one._nlookup))
                .collect::<Vec<_>>();

            debug!("batch_forget unique {} inodes {:?}", request.unique, inodes);

            fs.batch_forget(request, &inodes).await
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_fallocate(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let fallocate_in = match get_bincode_config().deserialize::<fuse_fallocate_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_fallocate_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(fallocate_in) => fallocate_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_fallocate"), async move {
            debug!(
                "fallocate unique {} inode {} {:?}",
                request.unique, in_header.nodeid, fallocate_in
            );

            let resp_value = if let Err(err) = fs
                .fallocate(
                    request,
                    in_header.nodeid,
                    fallocate_in.fh,
                    fallocate_in.offset,
                    fallocate_in.length,
                    fallocate_in.mode,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_readdirplus(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let readdirplus_in = match get_bincode_config().deserialize::<fuse_read_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_read_in in readdirplus failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(readdirplus_in) => readdirplus_in,
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_readdirplus"), async move {
            debug!(
                "readdirplus unique {} parent {} {:?}",
                request.unique, in_header.nodeid, readdirplus_in
            );

            let directory_plus = match fs
                .readdirplus(
                    request,
                    in_header.nodeid,
                    readdirplus_in.fh,
                    readdirplus_in.offset,
                    readdirplus_in.lock_owner,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(directory_plus) => directory_plus,
            };

            let max_size = readdirplus_in.size as usize;

            let mut entry_data = Vec::with_capacity(max_size);

            let entries = directory_plus.entries;
            let mut entries = pin!(entries);

            while let Some(entry) = entries.next().await {
                let entry = match entry {
                    Err(err) => {
                        reply_error_in_place(err, request, resp_sender).await;

                        return;
                    }

                    Ok(entry) => entry,
                };

                let name = &entry.name;

                let dir_entry_size = FUSE_DIRENTPLUS_SIZE + name.len();

                let padding_size = get_padding_size(dir_entry_size);

                if entry_data.len() + dir_entry_size > max_size {
                    break;
                }

                let attr = entry.attr;

                let dir_entry = fuse_direntplus {
                    entry_out: fuse_entry_out {
                        nodeid: attr.ino,
                        generation: entry.generation,
                        entry_valid: entry.entry_ttl.as_secs(),
                        attr_valid: entry.attr_ttl.as_secs(),
                        entry_valid_nsec: entry.entry_ttl.subsec_nanos(),
                        attr_valid_nsec: entry.attr_ttl.subsec_nanos(),
                        attr: attr.into(),
                    },
                    dirent: fuse_dirent {
                        ino: entry.inode,
                        off: entry.offset as u64,
                        namelen: name.len() as u32,
                        // learn from fuse-rs and golang bazil.org fuse DirentType
                        r#type: mode_from_kind_and_perm(entry.kind, 0) >> 12,
                    },
                };

                get_bincode_config()
                    .serialize_into(&mut entry_data, &dir_entry)
                    .expect("won't happened");

                entry_data.extend_from_slice(name.as_bytes());

                // padding
                entry_data.resize(entry_data.len() + padding_size, 0);
            }

            // TODO find a way to avoid multi allocate

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");

            let _ = resp_sender
                .send(Either::Right((data, entry_data.into())))
                .await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_rename2(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        mut data: &[u8],
        fs: &Arc<FS>,
    ) {
        let rename2_in = match get_bincode_config().deserialize::<fuse_rename2_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_rename2_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(rename2_in) => rename2_in,
        };

        data = &data[FUSE_RENAME2_IN_SIZE..];

        let (old_name, index) = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_rename2_in body doesn't have null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => (OsString::from_vec(data[..index].to_vec()), index),
        };

        data = &data[index + 1..];

        let new_name = match get_first_null_position(data) {
            None => {
                error!(
                    "fuse_rename2_in body doesn't have second null, request unique {}",
                    request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Some(index) => OsString::from_vec(data[..index].to_vec()),
        };

        let mut resp_sender = self.response_sender.clone();
        let fs = fs.clone();

        spawn(debug_span!("fuse_rename2"), async move {
            debug!(
                "rename2 unique {} parent {} name {:?} new parent {} new name {:?} flags {}",
                request.unique,
                in_header.nodeid,
                old_name,
                rename2_in.newdir,
                new_name,
                rename2_in.flags
            );

            let resp_value = if let Err(err) = fs
                .rename2(
                    request,
                    in_header.nodeid,
                    &old_name,
                    rename2_in.newdir,
                    &new_name,
                    rename2_in.flags,
                )
                .await
            {
                err.into()
            } else {
                0
            };

            let out_header = fuse_out_header {
                len: FUSE_OUT_HEADER_SIZE as u32,
                error: resp_value,
                unique: request.unique,
            };

            let data = get_bincode_config()
                .serialize(&out_header)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_lseek(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let mut resp_sender = self.response_sender.clone();

        let lseek_in = match get_bincode_config().deserialize::<fuse_lseek_in>(data) {
            Err(err) => {
                error!(
                    "deserialize fuse_lseek_in failed {}, request unique {}",
                    err, request.unique
                );

                reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                return;
            }

            Ok(lseek_in) => lseek_in,
        };

        let fs = fs.clone();

        spawn(debug_span!("fuse_lseek"), async move {
            debug!(
                "lseek unique {} inode {} {:?}",
                request.unique, in_header.nodeid, lseek_in
            );

            let reply_lseek = match fs
                .lseek(
                    request,
                    in_header.nodeid,
                    lseek_in.fh,
                    lseek_in.offset,
                    lseek_in.whence,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_lseek) => reply_lseek,
            };

            let lseek_out: fuse_lseek_out = reply_lseek.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_LSEEK_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &lseek_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }

    #[instrument(skip(self, data, fs))]
    async fn handle_copy_file_range(
        &mut self,
        request: Request,
        in_header: fuse_in_header,
        data: &[u8],
        fs: &Arc<FS>,
    ) {
        let mut resp_sender = self.response_sender.clone();

        let copy_file_range_in =
            match get_bincode_config().deserialize::<fuse_copy_file_range_in>(data) {
                Err(err) => {
                    error!(
                        "deserialize fuse_copy_file_range_in failed {}, request unique {}",
                        err, request.unique
                    );

                    reply_error_in_place(libc::EINVAL.into(), request, &self.response_sender).await;

                    return;
                }

                Ok(copy_file_range_in) => copy_file_range_in,
            };

        let fs = fs.clone();

        spawn(debug_span!("fuse_copy_file_range"), async move {
            debug!(
                "reply_copy_file_range unique {} inode {} {:?}",
                request.unique, in_header.nodeid, copy_file_range_in
            );

            let reply_copy_file_range = match fs
                .copy_file_range(
                    request,
                    in_header.nodeid,
                    copy_file_range_in.fh_in,
                    copy_file_range_in.off_in,
                    copy_file_range_in.nodeid_out,
                    copy_file_range_in.fh_out,
                    copy_file_range_in.off_out,
                    copy_file_range_in.len,
                    copy_file_range_in.flags,
                )
                .await
            {
                Err(err) => {
                    reply_error_in_place(err, request, resp_sender).await;

                    return;
                }

                Ok(reply_copy_file_range) => reply_copy_file_range,
            };

            let write_out: fuse_write_out = reply_copy_file_range.into();

            let out_header = fuse_out_header {
                len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                error: 0,
                unique: request.unique,
            };

            let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);

            get_bincode_config()
                .serialize_into(&mut data, &out_header)
                .expect("won't happened");
            get_bincode_config()
                .serialize_into(&mut data, &write_out)
                .expect("won't happened");

            let _ = resp_sender.send(Either::Left(data)).await;
        });
    }
}

async fn reply_error_in_place<S>(err: Errno, request: Request, sender: S)
where
    S: Sink<Either<Vec<u8>, (Vec<u8>, Bytes)>>,
{
    let out_header = fuse_out_header {
        len: FUSE_OUT_HEADER_SIZE as u32,
        error: err.into(),
        unique: request.unique,
    };

    let data = get_bincode_config()
        .serialize(&out_header)
        .expect("won't happened");

    let _ = pin!(sender).send(Either::Left(data)).await;
}

#[inline]
fn spawn<F>(span: Span, fut: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
    task::spawn(fut.instrument(span));

    #[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
    task::spawn(fut.instrument(span)).detach()
}
