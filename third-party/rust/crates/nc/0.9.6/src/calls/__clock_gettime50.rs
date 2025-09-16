/// Get time of specific clock.
pub unsafe fn __clock_gettime50(which_clock: clockid_t, tp: &mut timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *mut timespec_t as usize;
    syscall2(SYS___CLOCK_GETTIME50, which_clock, tp_ptr).map(drop)
}
