/// Suspend current thread for some time.
pub unsafe fn thr_suspend(timeout: &timespec_t) -> Result<(), Errno> {
    let timeout_ptr = timeout as *const timespec_t as usize;
    syscall1(SYS_THR_SUSPEND, timeout_ptr).map(drop)
}
