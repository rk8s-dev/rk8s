/// Receive a msg from a socket.
///
/// # Examples
///
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let mut iov = [nc::iovec_t {
///             iov_base: msg.as_ptr() as *const c_void,
///             iov_len: msg.len(),
///         }];
///         let msg_hdr = nc::msghdr_t {
///             msg_name: ptr::null(),
///             msg_namelen: 0,
///             msg_iov: iov.as_mut_ptr(),
///             msg_iovlen: iov.len(),
///             msg_control: ptr::null(),
///             msg_controllen: 0,
///             msg_flags: 0,
///         };
///         let ret = unsafe { nc::sendmsg(fds[WRITE_SIDE], &msg_hdr, 0) };
///         assert!(ret.is_ok());
///         assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [[0_u8; 64]; 4];
///     let mut iov = Vec::with_capacity(buf.len());
///     for ref mut item in (&mut buf).iter() {
///         iov.push(nc::iovec_t {
///             iov_base: item.as_ptr() as *const c_void,
///             iov_len: item.len(),
///         });
///     }
///     let mut msg_hdr = nc::msghdr_t {
///         msg_name: ptr::null(),
///         msg_namelen: 0,
///         msg_iov: iov.as_mut_ptr(),
///         msg_iovlen: iov.len(),
///         msg_control: ptr::null(),
///         msg_controllen: 0,
///         msg_flags: 0,
///     };
///     let ret = unsafe { nc::recvmsg(fds[READ_SIDE], &mut msg_hdr, 0) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap();
///     // We only read the first buffer block.
///     let msg = std::str::from_utf8(&buf[0][..nread as usize]).unwrap();
///     println!("[main] recv msg: {msg}");
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn recvmsg(sockfd: i32, msg: &mut msghdr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as *mut msghdr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_RECVMSG, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}
