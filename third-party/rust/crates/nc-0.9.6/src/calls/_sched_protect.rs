pub unsafe fn _sched_protect(priority: i32) -> Result<(), Errno> {
    let priority = priority as usize;
    syscall1(SYS__SCHED_PROTECT, priority).map(drop)
}
