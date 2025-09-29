use rfuse3::raw::{Filesystem, MountHandle};
use rfuse3::{MountOptions, raw::Session};
use std::ffi::{OsStr, OsString};

#[allow(unused)]
pub async fn mount_filesystem<F: Filesystem + std::marker::Sync + Send + 'static>(
    fs: F,
    mountpoint: &OsStr,
) -> MountHandle {
    //let logfs = LoggingFileSystem::new(fs);

    let mount_path: OsString = OsString::from(mountpoint);

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    // .allow_other(true)
    mount_options.force_readdir_plus(true).uid(uid).gid(gid);

    Session::<F>::new(mount_options)
        .mount(fs, mount_path)
        .await
        .unwrap()
}
