/// Mount filesystem.
pub unsafe fn fmount<P: AsRef<Path>>(
    fs_type: P,
    fd: i32,
    flags: i32,
    data: usize,
) -> Result<(), Errno> {
    let fs_type = CString::new(fs_type.as_ref());
    let fs_type_ptr = fs_type.as_ptr() as usize;
    let fd = fd as usize;
    let flags = flags as usize;
    syscall4(SYS_FMOUNT, fs_type_ptr, fd, flags, data).map(drop)
}
