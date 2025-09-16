/// Set feed-forward clock estimates.
pub unsafe fn ffclock_setestimate(cest: &mut ffclock_estimate_t) -> Result<(), Errno> {
    let cest_ptr = cest as *mut ffclock_estimate_t as usize;
    syscall1(SYS_FFCLOCK_SETESTIMATE, cest_ptr).map(drop)
}
