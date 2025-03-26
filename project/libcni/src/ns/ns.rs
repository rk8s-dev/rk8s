use std::fmt::{Display, Formatter};
use std::fs::{DirBuilder, File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, IntoRawFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use nix::sched::CloneFlags;

pub const BIND_MOUNT_PATH: &str = "/var/run/netns";

/// Represents a network namespace (netns).
///
/// This struct holds a file descriptor (`f`) that represents a network namespace.
#[derive(Debug)]
pub struct Netns {
    f: File,
    path: Option<PathBuf>,
}
impl Clone for Netns {
    /// Clones a `Netns` instance, creating a new file descriptor for the network namespace.
    ///
    /// # Returns
    /// A new `Netns` instance with a cloned file descriptor.
    fn clone(&self) -> Self {
        let new_file = self.f.try_clone().expect("Failed to clone file");
        Netns {
            f: new_file,
            path: self.path.clone(),
        }
    }
}
impl Netns {
    pub fn new() -> anyhow::Result<Self> {
        nix::sched::unshare(CloneFlags::CLONE_NEWNET)?;
        Self::get()
    }

    /// Creates a new network namespace with a given name.
    ///
    /// This function creates a new network namespace and binds it to a specific path.
    ///
    /// # Arguments
    /// * `name` - The name of the network namespace.
    ///
    /// # Returns
    /// A new `Netns` instance representing the newly created named namespace.
    ///
    /// # Errors
    /// This function returns an error if the namespace creation or bind mounting fails.
    pub fn new_named(name: &str) -> anyhow::Result<Self> {
        let bind_mount_path: &Path = Path::new(BIND_MOUNT_PATH);
        if !bind_mount_path.exists() {
            DirBuilder::new().mode(0o755).recursive(true).create(bind_mount_path)?;
        }

        let named_path = bind_mount_path.join(name);

        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o444)
            .open(&named_path)?;

        let new_ns = Self::new()?;
        let ns_path = format!("/proc/{}/task/{}/ns/net", std::process::id(), nix::unistd::gettid());
        nix::mount::mount(
            Some(Path::new(&ns_path)),
            &named_path,
            None::<&str>,
            nix::mount::MsFlags::MS_BIND,
            None::<&str>,
        )?;

        Ok(new_ns)
    }

    /// Deletes a named network namespace.
    ///
    /// This function unmounts and removes the file associated with the given namespace.
    ///
    /// # Arguments
    /// * `name` - The name of the network namespace to be deleted.
    ///
    /// # Returns
    /// A result indicating success or failure.
    pub fn delete_named(name: &str) -> anyhow::Result<()> {
        let named_path = Path::new(BIND_MOUNT_PATH).join(name);
        if named_path.exists() {
            nix::mount::umount2(&named_path, nix::mount::MntFlags::MNT_DETACH)?;
            std::fs::remove_file(named_path)?;
        } 
        Ok(())
    }

    /// Retrieves a network namespace from the specified path.
    ///
    /// # Arguments
    /// * `path` - The path to the network namespace file.
    ///
    /// # Returns
    /// An `Option` containing the `Netns` instance if the namespace exists.
    pub fn get_from_path(path: &Path) -> anyhow::Result<Option<Self>> {
        let file = OpenOptions::new().read(true).open(&path).ok();
        
        match file {
            None => Ok(None),
            Some(file) => {
                Ok(Some(Self { f: file, path: Some(path.to_path_buf()) }))
            },
        }
    }

    /// Retrieves a network namespace by its name.
    ///
    /// # Arguments
    /// * `name` - The name of the network namespace to retrieve.
    ///
    /// # Returns
    /// An `Option` containing the `Netns` instance if the namespace exists.
    pub fn get_from_name(name: &str) -> anyhow::Result<Option<Self>> {
        Self::get_from_path(&Path::new(BIND_MOUNT_PATH).join(name))
    }

    /// Retrieves the current network namespace.
    ///
    /// # Returns
    /// The `Netns` instance representing the current network namespace.
    ///
    /// # Errors
    /// This function returns an error if the current network namespace cannot be retrieved.
    pub fn get() -> anyhow::Result<Self> {
        let ns_path = format!("/proc/{}/task/{}/ns/net", std::process::id(), nix::unistd::gettid());
        let file = OpenOptions::new().read(true).open(Path::new(&ns_path))?;
        Ok(Self { f: file, path: Some(PathBuf::from(ns_path)) })
    }

    /// Sets the current process's network namespace to this one.
    ///
    /// # Returns
    /// A result indicating success or failure.
    pub fn set(&self) -> anyhow::Result<()> {
        nix::sched::setns(self.f.as_fd(), CloneFlags::CLONE_NEWNET).map_err(anyhow::Error::from)
    }
             
    /// Returns a unique identifier for the network namespace.
    ///
    /// The identifier is based on the device and inode of the namespace file.
    ///
    /// # Returns
    /// A string representing the unique identifier for the network namespace.
    pub fn unique_id(&self) -> String {
        match self.f.metadata() {
            Err(_) => {
                "NS(unknown)".into()
            }
            Ok(metadata) => {
                format!("NS({}:{})", metadata.dev(), metadata.ino())
            }
        }
    }
    /// Returns the raw file descriptor for the network namespace.
    ///
    /// # Returns
    /// The raw file descriptor representing the network namespace.
    pub fn as_fd(&self) -> RawFd {
        self.f.as_raw_fd()
    }

    /// Consumes the `Netns` instance and returns the raw file descriptor.
    ///
    /// # Returns
    /// The raw file descriptor representing the network namespace.
    pub fn into_fd(self) -> RawFd {
        self.f.into_raw_fd()
    }

    /// Retrieves the path associated with the network namespace.
    ///
    /// # Returns
    /// An `Option` containing the path if it exists.
    pub fn path(&self) -> Option<PathBuf> {
        self.path.clone()
    }
}

/// Executes a function in the context of a target network namespace asynchronously.
///
/// # Arguments
/// * `cur_ns` - The current network namespace.
/// * `target_ns` - The target network namespace to switch to.
/// * `exec` - The async function to execute within the target namespace.
///
/// # Returns
/// A result containing the output of the executed function.
pub async fn exec_netns<T,F>(cur_ns: &Netns, target_ns: &Netns, exec: F) -> Result<T,anyhow::Error>
where
    F: std::future::Future<Output = Result<T,anyhow::Error>>,
{
    target_ns.set()?;  
    let result = exec.await;  
    cur_ns.set()?;
    result
}

impl PartialEq<Self> for Netns {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        let self_meta = self.f.metadata();
        let other_meta = other.f.metadata();
        if self_meta.is_err() || other_meta.is_err() {
            return false;
        }
        let self_meta = self_meta.unwrap();
        let other_meta = other_meta.unwrap();
        return self_meta.dev() == other_meta.dev() && self_meta.ino() == other_meta.ino();
    }
}

impl Display for Netns {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.f.metadata() {
            Err(_) => {
                write!(f, "NS({}: unknown)", self.f.as_raw_fd())
            }
            Ok(metadata) => {
                write!(f, "NS({}: {}, {})", self.f.as_raw_fd(), metadata.dev(), metadata.ino())
            }
        }
    }
}