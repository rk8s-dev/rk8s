/// Get resource usage.
pub unsafe fn __getrusage50(who: i32, usage: &mut rusage_t) -> Result<(), Errno> {
    let who = who as usize;
    let usage_ptr = usage as *mut rusage_t as usize;
    syscall2(SYS___GETRUSAGE50, who, usage_ptr).map(drop)
}
