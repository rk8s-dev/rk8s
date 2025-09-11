/// Mount filesystem.
///
/// # Examples
///
/// ```
/// let target_dir = "/tmp/nc-mount";
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
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, target_dir, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mount<P: AsRef<Path>>(
    dev_name: P,
    dir_name: P,
    fs_type: P,
    flags: u32,
    data_page: *mut core::ffi::c_void,
) -> Result<(), Errno> {
    let dev_name = CString::new(dev_name.as_ref());
    let dev_name_ptr = dev_name.as_ptr() as usize;
    let dir_name = CString::new(dir_name.as_ref());
    let dir_name_ptr = dir_name.as_ptr() as usize;
    let fs_type = CString::new(fs_type.as_ref());
    let fs_type_ptr = fs_type.as_ptr() as usize;
    let flags = flags as usize;
    let data_page = data_page as usize;
    syscall5(
        SYS_MOUNT,
        dev_name_ptr,
        dir_name_ptr,
        fs_type_ptr,
        flags,
        data_page,
    )
    .map(drop)
}
