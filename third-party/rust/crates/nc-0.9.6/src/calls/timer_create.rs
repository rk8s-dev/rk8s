/// Create a per-process timer
///
/// # Examples
///
/// ```
/// let mut timerid = nc::timer_t::default();
/// let ret = unsafe { nc::timer_create(nc::CLOCK_MONOTONIC, None, &mut timerid) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timer_create(
    clock: clockid_t,
    event: Option<&mut sigevent_t>,
    timer_id: &mut timer_t,
) -> Result<(), Errno> {
    let clock = clock as usize;
    let event_ptr = event.map_or(core::ptr::null_mut::<sigevent_t>() as usize, |event| {
        event as *mut sigevent_t as usize
    });
    let timer_id_ptr = timer_id as *mut timer_t as usize;
    syscall3(SYS_TIMER_CREATE, clock, event_ptr, timer_id_ptr).map(drop)
}
