/// Create an endpoint for communication.
///
/// # Examples
///
/// ```
/// let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
/// assert!(socket_fd.is_ok());
/// let socket_fd = socket_fd.unwrap();
/// let ret = unsafe { nc::close(socket_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn socket(domain: i32, sock_type: i32, protocol: i32) -> Result<i32, Errno> {
    let domain = domain as usize;
    let sock_type = sock_type as usize;
    let protocol = protocol as usize;
    syscall3(SYS_SOCKET, domain, sock_type, protocol).map(|ret| ret as i32)
}
