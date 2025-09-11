/// Obtain a duplicate of another process's file descriptor.
///
/// # Examples
///
/// ```
/// const STDOUT_FD: i32 = 1;
///
/// let pid = unsafe { nc::fork() };
///
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// if pid == 0 {
///     println!("In child process, pid: {}", unsafe { nc::getpid() });
///     let path = "/tmp/nc-pidfdopen";
///     let fd = unsafe {
///         nc::openat(
///             nc::AT_FDCWD,
///             path,
///             nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///             0o644,
///         )
///     };
///     assert!(fd.is_ok());
///     let fd = fd.unwrap();
///     let ret = unsafe { nc::dup3(fd, STDOUT_FD, 0) };
///     assert!(ret.is_ok());
///     println!("[child] stdout redirected to file!");
///
///     let t = nc::timespec_t {
///         tv_sec: 2,
///         tv_nsec: 0,
///     };
///     unsafe {
///         let ret = nc::nanosleep(&t, None);
///         assert!(ret.is_ok());
///         let ret = nc::close(fd);
///         assert!(ret.is_ok());
///         let ret = nc::unlinkat(nc::AT_FDCWD, path, 0);
///         assert!(ret.is_ok());
///         nc::exit(0);
///     }
/// }
///
/// println!("[parent] child pid: {}", pid);
///
/// let t = nc::timespec_t {
///     tv_sec: 2,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
///
/// let pidfd = unsafe { nc::pidfd_open(pid, 0) };
/// if pidfd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let pidfd = pidfd.unwrap();
///
/// let child_stdout_fd = unsafe { nc::pidfd_getfd(pidfd, STDOUT_FD, 0) };
/// if child_stdout_fd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let child_stdout_fd = child_stdout_fd.unwrap();
/// let msg = b"Hello, msg from parent process\n";
/// let ret = unsafe { nc::write(child_stdout_fd, msg) };
/// assert!(ret.is_ok());
/// let nwrite = ret.unwrap();
/// assert_eq!(nwrite as usize, msg.len());
///
/// let ret = unsafe { nc::close(pidfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(child_stdout_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pidfd_getfd(pidfd: i32, target_fd: i32, flags: u32) -> Result<i32, Errno> {
    let pidfd = pidfd as usize;
    let target_fd = target_fd as usize;
    let flags = flags as usize;
    syscall3(SYS_PIDFD_GETFD, pidfd, target_fd, flags).map(|ret| ret as i32)
}
