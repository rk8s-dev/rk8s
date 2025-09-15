/// Wait for queued signals (REALTIME)
pub unsafe fn sigtimedwait(
    set: &sigset_t,
    info: &mut siginfo_t,
    timeout: &timespec_t,
) -> Result<i32, Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    let timeout_ptr = timeout as *const timespec_t as usize;
    syscall3(SYS_SIGTIMEDWAIT, set_ptr, info_ptr, timeout_ptr).map(|val| val as i32)
}
