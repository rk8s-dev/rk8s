/// Delete a per-process timer (REALTIME)
pub unsafe fn ktimer_delete(timer_id: i32) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    syscall1(SYS_KTIMER_DELETE, timer_id).map(drop)
}
