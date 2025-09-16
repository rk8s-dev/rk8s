/// Read/write system parameters.
pub unsafe fn _sysctl(args: &mut sysctl_args_t) -> Result<(), Errno> {
    let args_ptr = args as *mut sysctl_args_t as usize;
    syscall1(SYS__SYSCTL, args_ptr).map(drop)
}
