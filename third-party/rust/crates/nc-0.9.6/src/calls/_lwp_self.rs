/// Get thread id of current thread.
pub unsafe fn _lwp_self() -> Result<lwpid_t, Errno> {
    syscall0(SYS__LWP_SELF).map(|val| val as lwpid_t)
}
