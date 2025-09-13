/// Set the default FIB (routing table) for the calling process
pub unsafe fn setfib(fibnum: i32) -> Result<(), Errno> {
    let fibnum = fibnum as usize;
    syscall1(SYS_SETFIB, fibnum).map(drop)
}
