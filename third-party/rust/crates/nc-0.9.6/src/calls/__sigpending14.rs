/// Examine pending signals.
pub unsafe fn __sigpending14(set: &mut sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *mut sigset_t as usize;
    syscall1(SYS___SIGPENDING14, set_ptr).map(drop)
}
