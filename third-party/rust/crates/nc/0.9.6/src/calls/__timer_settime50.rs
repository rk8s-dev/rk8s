/// Arm/disarm state of per-process timer.
pub unsafe fn __timer_settime50(
    timer_id: timer_t,
    flags: i32,
    new_value: &itimerspec_t,
    old_value: Option<&mut itimerspec_t>,
) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let flags = flags as usize;
    let new_value_ptr = new_value as *const itimerspec_t as usize;
    let old_value_ptr = old_value.map_or(0, |old_value| old_value as *mut itimerspec_t as usize);
    syscall4(
        SYS___TIMER_SETTIME50,
        timer_id,
        flags,
        new_value_ptr,
        old_value_ptr,
    )
    .map(drop)
}
