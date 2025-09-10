/// Get resolution(precision) of the specific clock.
///
/// # Examples
///
/// ```
/// let mut tp = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_getres(nc::CLOCK_BOOTTIME, Some(&mut tp)) };
/// assert!(ret.is_ok());
/// assert!(tp.tv_nsec > 0);
/// ```
pub unsafe fn clock_getres(
    which_clock: clockid_t,
    tp: Option<&mut timespec_t>,
) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp.map_or(core::ptr::null_mut::<timespec_t>() as usize, |tp| {
        tp as *mut timespec_t as usize
    });
    syscall2(SYS_CLOCK_GETRES, which_clock, tp_ptr).map(drop)
}
