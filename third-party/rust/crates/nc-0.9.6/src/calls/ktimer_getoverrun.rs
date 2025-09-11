/// Returns the timer expiration overrun count as explained above.
pub unsafe fn ktimer_getoverrun(timer_id: i32) -> Result<i32, Errno> {
    let timer_id = timer_id as usize;
    syscall1(SYS_KTIMER_GETOVERRUN, timer_id).map(|val| val as i32)
}
