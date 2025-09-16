/// Fetch state of per-process timer>
pub unsafe fn __timer_gettime50(timer_id: timer_t, curr: &mut itimerspec_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let curr_ptr = curr as *mut itimerspec_t as usize;
    syscall2(SYS___TIMER_GETTIME50, timer_id, curr_ptr).map(drop)
}
