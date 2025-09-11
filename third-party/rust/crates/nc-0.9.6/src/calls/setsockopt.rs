/// Set options on sockets.
///
/// # Examples
///
/// ```
/// let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
/// assert!(socket_fd.is_ok());
/// let socket_fd = socket_fd.unwrap();
///
/// // Enable tcp fast open.
/// let queue_len: i32 = 5;
/// let ret = unsafe {
///     nc::setsockopt(
///         socket_fd,
///         nc::IPPROTO_TCP,
///         nc::TCP_FASTOPEN,
///         &queue_len as *const i32 as *const _,
///         std::mem::size_of_val(&queue_len) as nc::socklen_t,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(socket_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn setsockopt(
    sockfd: i32,
    level: i32,
    opt_name: i32,
    opt_val: *const core::ffi::c_void,
    opt_len: socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let opt_name = opt_name as usize;
    let opt_val = opt_val as usize;
    let opt_len = opt_len as usize;
    syscall5(SYS_SETSOCKOPT, sockfd, level, opt_name, opt_val, opt_len).map(drop)
}
