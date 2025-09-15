/// Wait for some event on file descriptors.
///
/// The `timeout` argument specifies the number of milliseconds that `poll()`
/// should block waiting for a file descriptor to become ready. Specifying
/// a timeout of zero causes `poll()` to return immediately, even if
/// no file descriptors are ready.
///
/// ## Return value
///
/// On success, it returns a nonnegative value which is the number of events
/// in the `fds` whose `revents` fields have been set to a nonzero value.
///
/// # Exampless
///
/// ```rust
/// use std::thread;
/// use std::time::Duration;
///
/// const STDIN_FD: i32 = 0;
/// const STDOUT_FD: i32 = 1;
///
/// fn main() {
///     let mut fds = [
///         nc::pollfd_t {
///             fd: STDIN_FD,
///             events: nc::POLLIN,
///             revents: 0,
///         },
///         nc::pollfd_t {
///             fd: STDOUT_FD,
///             events: nc::POLLOUT,
///             revents: 0,
///         },
///     ];
///
///     // Create a background thread to print some messages to stdout.
///     let handle = thread::spawn(|| {
///         thread::sleep(Duration::from_millis(100));
///         println!("MESSAGES from rust");
///     });
///
///     let ret = unsafe { nc::poll(&mut fds, 3000) };
///     assert!(ret.is_ok());
///     let num_ready = ret.unwrap();
///     println!("num of fds ready to read: {num_ready}");
///     assert!(handle.join().is_ok());
/// }
/// ```
pub unsafe fn poll(fds: &mut [pollfd_t], timeout_msecs: i32) -> Result<i32, Errno> {
    let fds_ptr = fds.as_mut_ptr() as usize;
    let nfds = fds.len();
    let timeout_msecs = timeout_msecs as usize;
    syscall3(SYS_POLL, fds_ptr, nfds, timeout_msecs).map(|ret| ret as i32)
}
