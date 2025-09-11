/// Get filesystem type information.
pub unsafe fn sysfs(option: i32, arg1: usize, arg2: usize) -> Result<i32, Errno> {
    let option = option as usize;
    syscall3(SYS_SYSFS, option, arg1, arg2).map(|ret| ret as i32)
}
