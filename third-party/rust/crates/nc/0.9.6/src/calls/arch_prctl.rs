/// Set architecture-specific thread state.
pub unsafe fn arch_prctl(code: i32, arg2: usize) -> Result<(), Errno> {
    let code = code as usize;
    syscall2(SYS_ARCH_PRCTL, code, arg2).map(drop)
}
