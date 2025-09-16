/// Select a set of signals
pub unsafe fn sigwait(set: &sigset_t, sig: &mut i32) -> Result<(), Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let sig_ptr = sig as *mut i32 as usize;
    syscall2(SYS_SIGWAIT, set_ptr, sig_ptr).map(drop)
}
