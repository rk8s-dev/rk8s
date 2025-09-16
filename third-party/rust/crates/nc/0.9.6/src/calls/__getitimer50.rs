/// Get value of an interval timer.
pub unsafe fn __getitimer50(which: i32, curr_val: &mut itimerval_t) -> Result<(), Errno> {
    let which = which as usize;
    let curr_val_ptr = curr_val as *mut itimerval_t as usize;
    syscall2(SYS___GETITIMER50, which, curr_val_ptr).map(drop)
}
