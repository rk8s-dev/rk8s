/// Unmount filesystem.
///
/// # Examples
///
/// ```
/// let target_dir = "/tmp/nc-umount2";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, target_dir, 0o755) };
/// assert!(ret.is_ok());
///
/// let src_dir = "/etc";
/// let fs_type = "";
/// let mount_flags = nc::MS_BIND | nc::MS_RDONLY;
/// let data = std::ptr::null_mut();
/// let ret = unsafe { nc::mount(src_dir, target_dir, fs_type, mount_flags, data) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
///
/// let flags = nc::UMOUNT_NOFOLLOW;
/// let ret = unsafe { nc::umount2(target_dir, flags) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
///
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, target_dir, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn umount2<P: AsRef<Path>>(name: P, flags: u32) -> Result<(), Errno> {
    // NOTE(Shaohua): type of flags is defined as i32 in linux kernel.
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_UMOUNT2, name_ptr, flags).map(drop)
}
