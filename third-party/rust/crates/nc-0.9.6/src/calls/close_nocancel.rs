/// Close a file descriptor.
pub unsafe fn close_nocancel(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_CLOSE_NOCANCEL, fd).map(drop)
}
