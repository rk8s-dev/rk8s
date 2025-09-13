/// Accept a connection on a socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18083;
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
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     let conn_fd = unsafe {
///         nc::accept4(
///             listen_fd,
///             Some(&mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t),
///             Some(&mut conn_addr_len),
///             nc::SOCK_CLOEXEC,
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn accept4(
    sockfd: i32,
    addr: Option<*mut sockaddr_t>,
    addrlen: Option<&mut socklen_t>,
    flags: i32,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr.map_or(core::ptr::null_mut::<sockaddr_t>() as usize, |addr| {
        addr as usize
    });
    let addrlen_ptr = addrlen.map_or(core::ptr::null_mut::<socklen_t>() as usize, |addrlen| {
        addrlen as *mut socklen_t as usize
    });
    let flags = flags as usize;
    syscall4(SYS_ACCEPT4, sockfd, addr_ptr, addrlen_ptr, flags).map(|val| val as i32)
}
