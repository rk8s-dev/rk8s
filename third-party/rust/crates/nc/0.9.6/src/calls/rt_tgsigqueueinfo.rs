/// Queue a signal and data.
pub unsafe fn rt_tgsigqueueinfo(
    tgid: pid_t,
    tid: pid_t,
    sig: i32,
    info: &mut siginfo_t,
) -> Result<(), Errno> {
    let tgid = tgid as usize;
    let tid = tid as usize;
    let sig = sig as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall4(SYS_RT_TGSIGQUEUEINFO, tgid, tid, sig, info_ptr).map(drop)
}
