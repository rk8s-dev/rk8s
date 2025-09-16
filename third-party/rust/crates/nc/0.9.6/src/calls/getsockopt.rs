/// Get options on sockets
///
/// # Examples
/// ```
/// use std::mem::size_of_val;
///
/// fn main() -> Result<(), nc::Errno> {
///     let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///
///     let set_queue_len: i32 = 5;
///     {
///         let buf_len = size_of_val(&set_queue_len) as nc::socklen_t;
///         let ret = unsafe {
///             nc::setsockopt(
///                 socket_fd,
///                 nc::IPPROTO_TCP,
///                 nc::TCP_FASTOPEN,
///                 &set_queue_len as *const i32 as *const _,
///                 buf_len,
///             )
///         };
///         assert!(ret.is_ok());
///     }
///
///     let mut get_queue_len: i32 = 0;
///     {
///         let mut buf_len = size_of_val(&get_queue_len) as nc::socklen_t;
///         let ret = unsafe {
///             nc::getsockopt(
///                 socket_fd,
///                 nc::IPPROTO_TCP,
///                 nc::TCP_FASTOPEN,
///                 &mut get_queue_len as *mut i32 as *mut _,
///                 &mut buf_len,
///             )
///         };
///         assert!(ret.is_ok());
///         println!("queue len: {get_queue_len}");
///     }
///     assert_eq!(set_queue_len, get_queue_len);
///
///     unsafe { nc::close(socket_fd) }
/// }
/// ```
pub unsafe fn getsockopt(
    sockfd: i32,
    level: i32,
    opt_name: i32,
    opt_val: *mut core::ffi::c_void,
    opt_len: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let opt_name = opt_name as usize;
    let opt_val_ptr = opt_val as usize;
    let opt_len_ptr = opt_len as *mut socklen_t as usize;
    syscall5(
        SYS_GETSOCKOPT,
        sockfd,
        level,
        opt_name,
        opt_val_ptr,
        opt_len_ptr,
    )
    .map(drop)
}
