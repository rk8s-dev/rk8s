/// High resolution sleep with a specific clock.
///
/// # Examples
///
/// ```
/// let t = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::clock_nanosleep(nc::CLOCK_MONOTONIC, 0, &t, None) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn clock_nanosleep(
    which_clock: clockid_t,
    flags: i32,
    request: &timespec_t,
    remain: Option<&mut timespec_t>,
) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let flags = flags as usize;
    let request_ptr = request as *const timespec_t as usize;
    let remain_ptr = remain.map_or(core::ptr::null_mut::<timespec_t>() as usize, |remain| {
        remain as *mut timespec_t as usize
    });
    syscall4(
        SYS_CLOCK_NANOSLEEP,
        which_clock,
        flags,
        request_ptr,
        remain_ptr,
    )
    .map(drop)
}
