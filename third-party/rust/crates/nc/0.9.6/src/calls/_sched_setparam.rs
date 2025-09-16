/// Set scheduling paramters.
pub unsafe fn _sched_setparam(
    pid: pid_t,
    lid: lwpid_t,
    policy: i32,
    param: &sched_param_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let policy = policy as usize;
    let param_ptr = param as *const sched_param_t as usize;
    syscall4(SYS__SCHED_SETPARAM, pid, lid, policy, param_ptr).map(drop)
}
