/// Architecture dependent system call.
pub unsafe fn sysarch(number: i32, args: usize) -> Result<usize, Errno> {
    let number = number as usize;
    syscall2(SYS_SYSARCH, number, args)
}
