/// Abort process with diagnostics
pub unsafe fn abort2(why: &str, args_len: i32, args: uinptr_t) -> ! {
    let why = CString::new(why);
    let why_ptr = why.as_ptr() as usize;
    let args_len = args_len as usize;
    let _ = syscall3(SYS_ABORT2, why_ptr, args_len, args);
    // Never returns
    unreachable!();
}
