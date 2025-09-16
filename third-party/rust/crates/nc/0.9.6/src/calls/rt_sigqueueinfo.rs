/// Queue a signal and data.
///
/// Send signal information to a thread.
pub unsafe fn rt_sigqueueinfo(pid: pid_t, sig: i32, info: &mut siginfo_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let sig = sig as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall3(SYS_RT_SIGQUEUEINFO, pid, sig, info_ptr).map(drop)
}
