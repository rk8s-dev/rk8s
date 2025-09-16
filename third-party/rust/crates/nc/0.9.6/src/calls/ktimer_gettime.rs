/// Stores the amount of time until the specified timer.
pub unsafe fn ktimer_gettime(timer_id: i32, value: &mut itimerspec_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let value_ptr = value as *mut itimerspec_t as usize;
    syscall2(SYS_KTIMER_GETTIME, timer_id, value_ptr).map(drop)
}
