/// Load a kernel module.
pub unsafe fn finit_module<P: AsRef<Path>>(
    fd: i32,
    param_values: P,
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let param_values = CString::new(param_values.as_ref());
    let param_values_ptr = param_values.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_FINIT_MODULE, fd, param_values_ptr, flags).map(drop)
}
