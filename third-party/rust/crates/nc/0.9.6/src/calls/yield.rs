/// Yield the processor.
pub unsafe fn r#yield() -> Result<(), Errno> {
    syscall0(SYS_YIELD).map(drop)
}
