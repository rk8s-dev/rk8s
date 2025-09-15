/// Send signal to specific thread.
pub unsafe fn thr_kill(id: isize, sig: i32) -> Result<(), Errno> {
    let id = id as usize;
    let sig = sig as usize;
    syscall2(SYS_THR_KILL, id, sig).map(drop)
}
