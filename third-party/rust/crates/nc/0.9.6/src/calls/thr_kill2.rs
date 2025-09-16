/// Send signal to specific thread.
pub unsafe fn thr_kill2(pid: pid_t, id: isize, sig: i32) -> Result<(), Errno> {
    let pid = pid as usize;
    let id = id as usize;
    let sig = sig as usize;
    syscall3(SYS_THR_KILL2, pid, id, sig).map(drop)
}
