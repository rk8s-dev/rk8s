/// Get resolution(precision) of the specific clock.
pub unsafe fn __clock_getres50(which_clock: clockid_t, tp: &mut timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *mut timespec_t as usize;
    syscall2(SYS___CLOCK_GETRES50, which_clock, tp_ptr).map(drop)
}
