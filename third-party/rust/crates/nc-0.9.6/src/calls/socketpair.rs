/// Create a pair of connected socket.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// let mut fds = [-1_i32; 2];
///
/// let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
/// assert!(ret.is_ok());
/// println!("socket pairs: {}, {}", fds[0], fds[1]);
///
/// // Start worker thread
/// thread::spawn(move || {
///     println!("worker thread started");
///     let msg = "Hello, Rust";
///     println!("[worker] Will send msg: {msg}");
///     let ret = unsafe { nc::write(fds[WRITE_SIDE], msg.as_bytes()) };
///     assert!(ret.is_ok());
///     assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// });
///
/// let mut buf = [0_u8; 1024];
/// let ret = unsafe { nc::read(fds[READ_SIDE], &mut buf) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// let msg = std::str::from_utf8(&buf[..nread as usize]).unwrap();
/// println!("[main] recv msg: {msg}");
///
/// unsafe {
///     let _ = nc::close(fds[0]);
///     let _ = nc::close(fds[1]);
/// }
/// ```
pub unsafe fn socketpair(
    domain: i32,
    type_: i32,
    protocol: i32,
    pair: &mut [i32; 2],
) -> Result<(), Errno> {
    let domain = domain as usize;
    let type_ = type_ as usize;
    let protocol = protocol as usize;
    let pair_ptr = pair.as_mut_ptr() as usize;
    syscall4(SYS_SOCKETPAIR, domain, type_, protocol, pair_ptr).map(drop)
}
