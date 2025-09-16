/// Disassociate parts of the process execution context
pub unsafe fn unshare(flags: i32) -> Result<(), Errno> {
    let flags = flags as usize;
    syscall1(SYS_UNSHARE, flags).map(drop)
}
