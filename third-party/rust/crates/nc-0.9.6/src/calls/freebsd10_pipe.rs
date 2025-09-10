/// Create a pipe.
pub unsafe fn freebsd10_pipe() -> Result<(), Errno> {
    syscall0(SYS_FREEBSD10_PIPE).map(drop)
}
