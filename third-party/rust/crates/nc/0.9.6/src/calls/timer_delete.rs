/// Delete a per-process timer
///
/// # Examples
///
/// ```
/// let mut timer_id = nc::timer_t::default();
/// let ret = unsafe { nc::timer_create(nc::CLOCK_MONOTONIC, None, &mut timer_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::timer_delete(timer_id) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timer_delete(timer_id: timer_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    syscall1(SYS_TIMER_DELETE, timer_id).map(drop)
}
