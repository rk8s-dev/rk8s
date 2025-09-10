/// Get file descriptor limit
pub unsafe fn getdtablesize() -> Result<i32, Errno> {
    syscall0(SYS_GETDTABLESIZE).map(|val| val as i32)
}
