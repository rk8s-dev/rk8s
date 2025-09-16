/// Returns the PGID(process group ID) of the process specified by `pid`.
///
/// # Examples
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// let pgid = unsafe { nc::getpgid(ppid) };
/// assert!(pgid.is_ok());
/// ```
pub unsafe fn getpgid(pid: pid_t) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    syscall1(SYS_GETPGID, pid).map(|ret| ret as pid_t)
}
