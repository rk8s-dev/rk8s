/// Flush all modified in-core data refered by `fd` to disk.
pub unsafe fn fsync_nocancel(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FSYNC_NOCANCEL, fd).map(drop)
}
