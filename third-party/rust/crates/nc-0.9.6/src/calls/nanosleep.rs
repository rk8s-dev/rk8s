/// High resolution sleep.
///
/// # Examples
///
/// ```
/// let t = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn nanosleep(req: &timespec_t, rem: Option<&mut timespec_t>) -> Result<(), Errno> {
    let req_ptr = req as *const timespec_t as usize;
    let rem_ptr = rem.map_or(core::ptr::null_mut::<timespec_t>() as usize, |rem| {
        rem as *mut timespec_t as usize
    });
    syscall2(SYS_NANOSLEEP, req_ptr, rem_ptr).map(drop)
}
