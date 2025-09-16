/// Examine and change blocked signals.
pub unsafe fn __sigprocmask14(
    how: i32,
    newset: &mut sigset_t,
    oldset: &mut sigset_t,
) -> Result<(), Errno> {
    let how = how as usize;
    let newset_ptr = newset as *mut sigset_t as usize;
    let oldset_ptr = oldset as *mut sigset_t as usize;
    syscall3(SYS___SIGPROCMASK14, how, newset_ptr, oldset_ptr).map(drop)
}
