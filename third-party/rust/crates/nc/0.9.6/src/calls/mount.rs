/// Mount filesystem.
pub unsafe fn mount<P: AsRef<Path>>(
    fs_type: P,
    dir_name: P,
    flags: i32,
    data: usize,
) -> Result<(), Errno> {
    let fs_type = CString::new(fs_type.as_ref());
    let fs_type_ptr = fs_type.as_ptr() as usize;
    let dir_name = CString::new(dir_name.as_ref());
    let dir_name_ptr = dir_name.as_ptr() as usize;
    let flags = flags as usize;
    syscall4(SYS_MOUNT, fs_type_ptr, dir_name_ptr, flags, data).map(drop)
}
