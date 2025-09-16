/// Signal a process through a pidfd.
///
/// @pidfd:  file descriptor of the process
/// @sig:    signal to send
/// @info:   signal info
/// @flags:  future flags
///
/// The syscall currently only signals via `PIDTYPE_PID` which covers
/// `kill(<positive-pid>, <signal>)`. It does not signal threads or process
/// groups.
/// In order to extend the syscall to threads and process groups the @flags
/// argument should be used. In essence, the @flags argument will determine
/// what is signaled and not the file descriptor itself. Put in other words,
/// grouping is a property of the flags argument not a property of the file
/// descriptor.
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
///     let curr_pid = unsafe { nc::getpid() };
///     println!("In child process, pid: {}", curr_pid);
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
/// assert!(pidfd.is_ok());
/// let pidfd = pidfd.unwrap();
///
/// let ret = unsafe { nc::pidfd_getfd(pidfd, STDOUT_FD, 0) };
/// println!("ret: {:?}", ret);
/// if let Err(errno) = ret {
///     eprintln!("pidfd_getfd() failed, err: {}", nc::strerror(errno));
/// }
/// let child_stdout_fd = ret.unwrap();
/// let msg = b"Hello, msg from parent process\n";
/// let ret = unsafe { nc::write(child_stdout_fd, msg) };
/// assert!(ret.is_ok());
/// let nwrite = ret.unwrap();
/// assert_eq!(nwrite as usize, msg.len());
///
/// let ret = unsafe { nc::pidfd_send_signal(pidfd, nc::SIGKILL, None, 0) };
/// println!("ret: {:?}", ret);
/// if let Err(errno) = ret {
///     eprintln!("pidfd_send_signal() failed, err: {}", nc::strerror(errno));
/// }
///
/// unsafe {
///     let ret = nc::close(pidfd);
///     assert!(ret.is_ok());
///     let ret = nc::close(child_stdout_fd);
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn pidfd_send_signal(
    pidfd: i32,
    sig: i32,
    info: Option<&mut siginfo_t>,
    flags: u32,
) -> Result<(), Errno> {
    let pidfd = pidfd as usize;
    let sig = sig as usize;
    let info_ptr = info.map_or(core::ptr::null_mut::<siginfo_t>() as usize, |info| {
        info as *mut siginfo_t as usize
    });
    let flags = flags as usize;
    syscall4(SYS_PIDFD_SEND_SIGNAL, pidfd, sig, info_ptr, flags).map(drop)
}
