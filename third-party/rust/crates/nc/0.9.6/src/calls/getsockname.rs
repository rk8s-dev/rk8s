/// Get current address to which the socket `sockfd` is bound.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18088;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(listen_fd, None, None, nc::SOCK_CLOEXEC)?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     unsafe {
///         let _ = nc::getsockname(conn_fd,
///             &mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t,
///             &mut conn_addr_len
///         );
///     }
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
pub unsafe fn getsockname(
    sockfd: i32,
    addr: *mut sockaddr_t,
    addrlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_GETSOCKNAME, sockfd, addr_ptr, addrlen_ptr).map(drop)
}
