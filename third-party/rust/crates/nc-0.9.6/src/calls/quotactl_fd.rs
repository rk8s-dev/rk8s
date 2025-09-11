/// Manipulate disk quotes.
pub unsafe fn quotactl_fd<P: AsRef<Path>>(
    fd: i32,
    cmd: u32,
    id: qid_t,
    addr: *const core::ffi::c_void,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let id = id as usize;
    let addr = addr as usize;
    syscall4(SYS_QUOTACTL_FD, fd, cmd, id, addr).map(drop)
}
