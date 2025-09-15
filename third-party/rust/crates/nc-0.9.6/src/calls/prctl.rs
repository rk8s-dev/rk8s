/// Operations on a process.
pub unsafe fn prctl(
    option: i32,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> Result<i32, Errno> {
    let option = option as usize;
    syscall5(SYS_PRCTL, option, arg2, arg3, arg4, arg5).map(|ret| ret as i32)
}
