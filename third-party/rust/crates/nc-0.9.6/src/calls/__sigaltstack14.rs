/// Get/set signal stack context.
pub unsafe fn __sigaltstack14(uss: &sigaltstack_t, uoss: &mut sigaltstack_t) -> Result<(), Errno> {
    let uss_ptr = uss as *const sigaltstack_t as usize;
    let uoss_ptr = uoss as *mut sigaltstack_t as usize;
    syscall2(SYS___SIGALTSTACK14, uss_ptr, uoss_ptr).map(drop)
}
