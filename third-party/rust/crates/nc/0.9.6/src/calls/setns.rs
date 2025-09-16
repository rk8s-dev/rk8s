/// Reassociate thread with a namespace.
pub unsafe fn setns(fd: i32, nstype: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let nstype = nstype as usize;
    syscall2(SYS_SETNS, fd, nstype).map(drop)
}
