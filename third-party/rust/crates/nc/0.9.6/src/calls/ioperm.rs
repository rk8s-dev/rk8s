/// Set port input/output permissions.
pub unsafe fn ioperm(from: usize, num: usize, turn_on: i32) -> Result<(), Errno> {
    let turn_on = turn_on as usize;
    syscall3(SYS_IOPERM, from, num, turn_on).map(drop)
}
