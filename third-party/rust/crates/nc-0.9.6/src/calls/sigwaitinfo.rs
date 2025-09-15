/// Wait for queued signals (REALTIME)
pub unsafe fn sigwaitinfo(set: &sigset_t, info: &mut siginfo_t) -> Result<i32, Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall2(SYS_SIGWAITINFO, set_ptr, info_ptr).map(|val| val as i32)
}
