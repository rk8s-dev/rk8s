/// Send signal to a process.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
/// if pid == 0 {
///     // child process.
///     let args = [""];
///     let env = [""];
///     let ret = unsafe { nc::execve("/usr/bin/yes", &args, &env) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let ret = unsafe { nc::kill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn kill(pid: pid_t, signal: i32) -> Result<(), Errno> {
    let pid = pid as usize;
    let signal = signal as usize;
    syscall2(SYS_KILL, pid, signal).map(drop)
}
