/// Manipulate disk quotes.
pub unsafe fn __quotactl<P: AsRef<Path>>(path: P, args: &mut quotactl_args_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let args_ptr = args as *mut quotactl_args_t as usize;
    syscall2(SYS___QUOTACTL, path_ptr, args_ptr).map(drop)
}
