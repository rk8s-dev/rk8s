/// Transfer data between two file descriptors.
///
/// # Examples
///
/// ```
/// println!("S_IRUSR: {}", nc::S_IRUSR);
/// println!("S_IRGRP: {}", nc::S_IRGRP);
///
/// let in_filename = "/etc/passwd";
/// let out_filename = "/tmp/passwd.copy";
///
/// #[cfg(any(target_os = "linux", target_os = "android"))]
/// let in_fd = unsafe {
///     nc::openat(nc::AT_FDCWD, in_filename, nc::O_RDONLY, 0).expect("Failed to open file!")
/// };
///
/// #[cfg(target_os = "freebsd")]
/// let in_fd = unsafe {
///     nc::openat(nc::AT_FDCWD, in_filename, nc::O_RDONLY, 0).expect("Failed to open file!")
/// };
///
/// #[cfg(any(target_os = "linux", target_os = "android"))]
/// let out_fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_WRONLY | nc::O_CREAT,
///         nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
///     )
///     .expect("Failed to open passwd copy file")
/// };
///
/// #[cfg(target_os = "freebsd")]
/// let out_fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_WRONLY | nc::O_CREAT,
///         nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
///     )
///     .expect("Failed to open passwd copy file")
/// };
///
/// let mut stat = nc::stat_t::default();
/// unsafe { nc::fstat(in_fd, &mut stat).expect("Failed to get file stat!") };
/// println!("stat: {:?}", stat);
///
/// let count = stat.st_blksize as usize;
/// println!("count: {}", count);
/// let nread =
///     unsafe { nc::sendfile(out_fd, in_fd, None, count).expect("Failed to call sendfile()") };
/// println!("nread: {}", nread);
///
/// unsafe {
///     let _ = nc::close(in_fd);
///     let _ = nc::close(out_fd);
/// }
///
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, out_filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sendfile(
    out_fd: i32,
    in_fd: i32,
    offset: Option<&mut off_t>,
    count: size_t,
) -> Result<ssize_t, Errno> {
    let out_fd = out_fd as usize;
    let in_fd = in_fd as usize;
    let offset_ptr = offset.map_or(core::ptr::null_mut::<off_t>() as usize, |offset| {
        offset as *mut off_t as usize
    });
    syscall4(SYS_SENDFILE, out_fd, in_fd, offset_ptr, count).map(|ret| ret as ssize_t)
}
