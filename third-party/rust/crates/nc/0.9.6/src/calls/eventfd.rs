/// Create a file descriptor for event notification.
pub unsafe fn eventfd(count: u32) -> Result<i32, Errno> {
    let count = count as usize;
    syscall1(SYS_EVENTFD, count).map(|ret| ret as i32)
}
