/// Copy a range of data from one file to another.
///
/// # Examples
///
/// ```
/// let path_in = "/tmp/nc-copy-file-range.in";
/// let fd_in = unsafe { nc::openat(nc::AT_FDCWD, path_in, nc::O_RDWR | nc::O_CREAT, 0o644) };
/// assert!(fd_in.is_ok());
/// let fd_in = fd_in.unwrap();
/// let msg = b"Hello, rust";
/// let ret = unsafe { nc::write(fd_in, msg) };
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let path_out = "/tmp/nc-copy-file-range.out";
/// let fd_out = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd_out.is_ok());
/// let fd_out = fd_out.unwrap();
/// let mut off_in = 0;
/// let mut off_out = 0;
/// let copy_len = 64;
/// let ret = unsafe { nc::copy_file_range(fd_in, &mut off_in, fd_out, &mut off_out, copy_len, 0) };
/// println!("ret: {ret:?}");
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd_in) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd_out) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_in, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn copy_file_range(
    fd_in: i32,
    off_in: &mut loff_t,
    fd_out: i32,
    off_out: &mut loff_t,
    len: size_t,
    flags: u32,
) -> Result<ssize_t, Errno> {
    let fd_in = fd_in as usize;
    let off_in_ptr = off_in as *mut loff_t as usize;
    let fd_out = fd_out as usize;
    let off_out_ptr = off_out as *mut loff_t as usize;
    let flags = flags as usize;
    syscall6(
        SYS_COPY_FILE_RANGE,
        fd_in,
        off_in_ptr,
        fd_out,
        off_out_ptr,
        len,
        flags,
    )
    .map(|ret| ret as ssize_t)
}
