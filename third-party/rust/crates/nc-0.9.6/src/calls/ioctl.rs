/// Control device.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-ioctl";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut attr: i32 = 0;
/// let cmd = nc::FS_IOC_GETFLAGS;
/// let ret = unsafe { nc::ioctl(fd, cmd, &mut attr as *mut i32 as *const _) };
/// assert!(ret.is_ok());
/// println!("attr: {}", attr);
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ioctl(fd: i32, cmd: u32, arg: *const core::ffi::c_void) -> Result<i32, Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let arg = arg as usize;
    syscall3(SYS_IOCTL, fd, cmd, arg).map(|ret| ret as i32)
}
