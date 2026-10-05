//! Real passthrough I/O with injected copy-up failures; no privileged mount.

use super::*;
use async_trait::async_trait;
use asyncfuse::raw::prelude::*;
use bytes::Bytes;
use std::sync::atomic::{AtomicI64, AtomicUsize};
use tokio::sync::Notify;

const BLOCK: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum Fault {
    None,
    ShortWrite,
    ZeroWrite,
    NoSpace,
    LateRead,
    CancelRead,
}

struct FaultLayer {
    inner: crate::passthrough::PassthroughFs,
    fault: Fault,
    active: AtomicI64,
    writes: AtomicUsize,
    entered: Notify,
    resume: Notify,
}

impl Filesystem for FaultLayer {
    async fn lookup(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
    ) -> asyncfuse::Result<ReplyEntry> {
        Filesystem::lookup(&self.inner, req, parent, name).await
    }

    async fn getattr(
        &self,
        req: Request,
        inode: Inode,
        fh: Option<u64>,
        flags: u32,
    ) -> asyncfuse::Result<ReplyAttr> {
        Filesystem::getattr(&self.inner, req, inode, fh, flags).await
    }

    async fn forget(&self, req: Request, inode: Inode, count: u64) {
        Filesystem::forget(&self.inner, req, inode, count).await;
    }

    async fn open(&self, req: Request, inode: Inode, flags: u32) -> asyncfuse::Result<ReplyOpen> {
        let opened = Filesystem::open(&self.inner, req, inode, flags).await?;
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(opened)
    }

    async fn read(
        &self,
        req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> asyncfuse::Result<ReplyData> {
        if offset >= BLOCK as u64 {
            match self.fault {
                Fault::LateRead => return Err(Error::from_raw_os_error(libc::EIO).into()),
                Fault::CancelRead => {
                    self.entered.notify_one();
                    self.resume.notified().await;
                }
                _ => {}
            }
        }
        Filesystem::read(&self.inner, req, inode, fh, offset, size.min(BLOCK as u32)).await
    }

    async fn write(
        &self,
        req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        data: &[u8],
        write_flags: u32,
        flags: u32,
    ) -> asyncfuse::Result<ReplyWrite> {
        let call = self.writes.fetch_add(1, Ordering::SeqCst);
        let len = match self.fault {
            Fault::ShortWrite => data.len().min(4093),
            Fault::ZeroWrite => return Ok(ReplyWrite { written: 0 }),
            Fault::NoSpace if call > 0 => return Err(Error::from_raw_os_error(libc::ENOSPC).into()),
            _ => data.len(),
        };
        Filesystem::write(
            &self.inner,
            req,
            inode,
            fh,
            offset,
            &data[..len],
            write_flags,
            flags,
        )
        .await
    }

    async fn release(
        &self,
        req: Request,
        inode: Inode,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> asyncfuse::Result<()> {
        let result =
            Filesystem::release(&self.inner, req, inode, fh, flags, lock_owner, flush).await;
        if result.is_ok() {
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
        result
    }
}

#[async_trait]
impl Layer for FaultLayer {
    fn root_inode(&self) -> Inode {
        1
    }

    async fn getattr_with_mapping(
        &self,
        inode: Inode,
        handle: Option<u64>,
        mapping: bool,
    ) -> Result<(libc::stat64, std::time::Duration)> {
        Layer::getattr_with_mapping(&self.inner, inode, handle, mapping).await
    }

    async fn create_with_context(
        &self,
        ctx: crate::context::OperationContext,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> asyncfuse::Result<ReplyCreated> {
        let created =
            Layer::create_with_context(&self.inner, ctx, parent, name, mode, flags).await?;
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(created)
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    lower: Arc<FaultLayer>,
    upper: Arc<FaultLayer>,
    overlay: Arc<OverlayFs>,
    node: Arc<OverlayInode>,
    _parent: Arc<OverlayInode>,
    expected: Bytes,
}

fn request() -> Request {
    Request {
        uid: unsafe { libc::geteuid() },
        gid: unsafe { libc::getegid() },
        pid: std::process::id(),
        ..Request::default()
    }
}

async fn make_layer(path: std::path::PathBuf, fault: Fault) -> Arc<FaultLayer> {
    let inner = crate::passthrough::PassthroughFs::new(crate::passthrough::config::Config {
        root_dir: path,
        do_import: true,
        writeback: false,
        ..Default::default()
    })
    .unwrap();
    inner.import().await.unwrap();
    Arc::new(FaultLayer {
        inner,
        fault,
        active: AtomicI64::new(0),
        writes: AtomicUsize::new(0),
        entered: Notify::new(),
        resume: Notify::new(),
    })
}

async fn fixture(lower_fault: Fault, upper_fault: Fault) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let lower_path = temp.path().join("lower");
    let upper_path = temp.path().join("upper");
    std::fs::create_dir(&lower_path).unwrap();
    std::fs::create_dir(&upper_path).unwrap();
    let expected = Bytes::from(
        (0..2 * BLOCK + 17)
            .map(|i| ((i * 37 + i / 251) % 251) as u8)
            .collect::<Vec<_>>(),
    );
    std::fs::write(lower_path.join("file"), &expected).unwrap();
    let lower = make_layer(lower_path, lower_fault).await;
    let upper = make_layer(upper_path, upper_fault).await;
    let parent_attr = Filesystem::getattr(upper.as_ref(), request(), 1, None, 0)
        .await
        .unwrap();
    let parent = Arc::new(
        OverlayInode::new_from_real_inode(
            "",
            1,
            String::new(),
            RealInode {
                layer: upper.clone(),
                in_upper_layer: true,
                inode: 1,
                whiteout: false,
                opaque: false,
                stat: Some(parent_attr),
            },
        )
        .await,
    );
    let file = Filesystem::lookup(lower.as_ref(), request(), 1, OsStr::new("file"))
        .await
        .unwrap();
    let node = Arc::new(
        OverlayInode::new_from_real_inode(
            "file",
            2,
            "file".into(),
            RealInode {
                layer: lower.clone(),
                in_upper_layer: false,
                inode: file.attr.ino,
                whiteout: false,
                opaque: false,
                stat: Some(ReplyAttr {
                    ttl: file.ttl,
                    attr: file.attr,
                }),
            },
        )
        .await,
    );
    *node.parent.lock().await = Arc::downgrade(&parent);
    let overlay = Arc::new(
        OverlayFs::new(
            Some(upper.clone()),
            vec![lower.clone()],
            Config::default(),
            1,
        )
        .unwrap(),
    );
    Fixture {
        temp,
        lower,
        upper,
        overlay,
        node,
        _parent: parent,
        expected,
    }
}

impl Fixture {
    fn assert_source_and_handles(&self) {
        assert_eq!(
            std::fs::read(self.temp.path().join("lower/file")).unwrap(),
            self.expected.as_ref()
        );
        assert_eq!(
            self.lower.active.load(Ordering::SeqCst),
            0,
            "lower handle must be released"
        );
        assert_eq!(
            self.upper.active.load(Ordering::SeqCst),
            0,
            "upper handle must be released"
        );
    }

    fn assert_no_final(&self) {
        assert!(
            !self.temp.path().join("upper/file").exists(),
            "failed copy-up must not publish a final name"
        );
        assert_eq!(
            std::fs::read_dir(self.temp.path().join("upper"))
                .unwrap()
                .count(),
            0,
            "private staging must not enter the upper namespace"
        );
    }
}

#[tokio::test]
async fn copy_up_short_writes_complete_the_real_file_without_panicking() {
    let f = fixture(Fault::None, Fault::ShortWrite).await;
    f.overlay
        .copy_regfile_up(request(), f.node.clone())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(f.temp.path().join("upper/file")).unwrap(),
        f.expected.as_ref()
    );
    assert!(f.upper.writes.load(Ordering::SeqCst) > 3);
    f.assert_source_and_handles();
}

#[tokio::test]
async fn copy_up_zero_progress_returns_eio_and_publishes_nothing() {
    let f = fixture(Fault::None, Fault::ZeroWrite).await;
    let error = f
        .overlay
        .copy_regfile_up(request(), f.node.clone())
        .await
        .err()
        .unwrap();
    assert_eq!(error.raw_os_error(), Some(libc::EIO));
    f.assert_no_final();
    f.assert_source_and_handles();
}

#[tokio::test]
async fn copy_up_enospc_after_a_real_write_returns_enospc_and_publishes_nothing() {
    let f = fixture(Fault::None, Fault::NoSpace).await;
    let error = f
        .overlay
        .copy_regfile_up(request(), f.node.clone())
        .await
        .err()
        .unwrap();
    assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
    f.assert_no_final();
    f.assert_source_and_handles();
}

#[tokio::test]
async fn copy_up_late_lower_failure_publishes_nothing_and_closes_actual_handles() {
    let f = fixture(Fault::LateRead, Fault::None).await;
    let error = f
        .overlay
        .copy_regfile_up(request(), f.node.clone())
        .await
        .err()
        .unwrap();
    assert_eq!(error.raw_os_error(), Some(libc::EIO));
    f.assert_no_final();
    f.assert_source_and_handles();
}

#[tokio::test]
async fn cancelled_copy_up_never_publishes_a_partial_file() {
    let f = fixture(Fault::CancelRead, Fault::None).await;
    let overlay = f.overlay.clone();
    let node = f.node.clone();
    let task = tokio::spawn(async move { overlay.copy_regfile_up(request(), node).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        f.lower.entered.notified(),
    )
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.assert_no_final();
    f.assert_source_and_handles();
}
