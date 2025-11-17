// Test-only mock layer for simulating rename2 behavior in tests.

use rfuse3::raw::Request;
use rfuse3::raw::reply::*;
use rfuse3::{Inode, Result as RfuseResult};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;

use crate::passthrough::PassthroughFs;

/// Type alias for rename2 override function
type Rename2OverrideFn = Arc<dyn Fn(Request, u64, &str, u64, &str, u32) -> RfuseResult<()> + Send + Sync>;

#[cfg(test)]
#[derive(Clone, Debug)]
pub enum RenameBehavior {
    Ok,
    Errno(i32),
    #[allow(dead_code)]
    DelayOk(Duration),
}

#[cfg(test)]
pub struct MockLayer {
    inner: Arc<PassthroughFs>,
    behavior: std::sync::Mutex<RenameBehavior>,
}

#[cfg(test)]
impl MockLayer {
    pub fn new_from_passthrough(inner: Arc<PassthroughFs>, b: RenameBehavior) -> Self {
        Self {
            inner,
            behavior: std::sync::Mutex::new(b),
        }
    }

    pub fn new(b: RenameBehavior) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir for MockLayer");
        let args = crate::passthrough::PassthroughArgs {
            root_dir: tmp.path().to_path_buf(),
            mapping: None::<&str>,
        };
        let fs = futures::executor::block_on(crate::passthrough::new_passthroughfs_layer(args))
            .expect("passthrough fs");
        Self::new_from_passthrough(Arc::new(fs), b)
    }

    #[allow(dead_code)]
    pub fn set_behavior(&self, b: RenameBehavior) {
        let mut g = self.behavior.lock().unwrap();
        *g = b;
    }

    /// Generate closure for rename2_override hook
    pub fn make_rename2_closure(self: Arc<Self>) -> Rename2OverrideFn {
        use rfuse3::raw::Filesystem as _;

        Arc::new(
            move |req: Request,
                  parent: u64,
                  name: &str,
                  new_parent: u64,
                  new_name: &str,
                  flags: u32| {
                let b = { self.behavior.lock().unwrap().clone() };
                match b {
                    RenameBehavior::Ok => futures::executor::block_on(self.inner.rename2(
                        req,
                        parent,
                        OsStr::new(name),
                        new_parent,
                        OsStr::new(new_name),
                        flags,
                    )),
                    RenameBehavior::Errno(e) => Err(std::io::Error::from_raw_os_error(e).into()),
                    RenameBehavior::DelayOk(dur) => {
                        std::thread::sleep(dur);
                        futures::executor::block_on(self.inner.rename2(
                            req,
                            parent,
                            OsStr::new(name),
                            new_parent,
                            OsStr::new(new_name),
                            flags,
                        ))
                    }
                }
            },
        )
    }
}

// Implement Layer for MockLayer by delegating root_inode
#[cfg(test)]
impl crate::overlayfs::layer::Layer for MockLayer {
    fn root_inode(&self) -> Inode {
        self.inner.root_inode()
    }
}

// Implement the rfuse3 Filesystem trait for MockLayer. We only override
// `rename2` to inject behavior; other methods use the default impls from the
// trait (returning ENOSYS), or the inner PassthroughFs when callers delegate.
#[cfg(test)]
impl rfuse3::raw::Filesystem for MockLayer {
    async fn init(&self, req: Request) -> RfuseResult<ReplyInit> {
        self.inner.init(req).await
    }

    async fn destroy(&self, req: Request) {
        let _ = self.inner.destroy(req).await;
    }

    async fn rename2(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        new_parent: Inode,
        new_name: &OsStr,
        flags: u32,
    ) -> RfuseResult<()> {
        let b = { self.behavior.lock().unwrap().clone() };
        match b {
            RenameBehavior::Ok => {
                self.inner
                    .rename2(req, parent, name, new_parent, new_name, flags)
                    .await
            }
            RenameBehavior::Errno(e) => Err(std::io::Error::from_raw_os_error(e).into()),
            RenameBehavior::DelayOk(dur) => {
                tokio::time::sleep(dur).await;
                self.inner
                    .rename2(req, parent, name, new_parent, new_name, flags)
                    .await
            }
        }
    }
}
