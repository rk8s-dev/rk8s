/// Create a file descriptor for event notification.
pub unsafe fn eventfd2(count: u32, flags: i32) -> Result<i32, Errno> {
    let count = count as usize;
    let flags = flags as usize;
    syscall2(SYS_EVENTFD2, count, flags).map(|ret| ret as i32)
}
