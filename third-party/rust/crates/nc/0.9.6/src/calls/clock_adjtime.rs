/// Tune kernel clock.
///
/// Returns clock state on success.
///
/// # Examples
///
/// ```
/// let mut tm = nc::timex_t::default();
/// let ret = unsafe { nc::clock_adjtime(nc::CLOCK_REALTIME, &mut tm) };
/// assert!(ret.is_ok());
/// assert!(tm.time.tv_sec > 1611552896);
/// ```
pub unsafe fn clock_adjtime(which_clock: clockid_t, tx: &mut timex_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tx_ptr = tx as *mut timex_t as usize;
    syscall2(SYS_CLOCK_ADJTIME, which_clock, tx_ptr).map(drop)
}
