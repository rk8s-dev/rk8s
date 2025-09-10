/// Set time of specific clock.
pub unsafe fn __clock_settime50(which_clock: clockid_t, tp: &timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *const timespec_t as usize;
    syscall2(SYS___CLOCK_SETTIME50, which_clock, tp_ptr).map(drop)
}
