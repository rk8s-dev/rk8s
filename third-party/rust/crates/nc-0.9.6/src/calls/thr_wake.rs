/// Notify thread wakeup from suspend state.
pub unsafe fn thr_wake(id: isize) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS_THR_WAKE, id).map(drop)
}
