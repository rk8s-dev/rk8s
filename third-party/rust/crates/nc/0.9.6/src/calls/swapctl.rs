/// Modify swap configuration
pub unsafe fn swapctl(cmd: i32, arg: usize, misc: i32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let misc = misc as usize;
    syscall3(SYS_SWAPCTL, cmd, arg, misc).map(|val| val as i32)
}
