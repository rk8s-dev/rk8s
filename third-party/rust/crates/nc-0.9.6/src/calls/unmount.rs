/// Unmount filesystem.
pub unsafe fn unmount<P: AsRef<Path>>(path: P, flags: i32) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_UNMOUNT, path_ptr, flags).map(drop)
}
