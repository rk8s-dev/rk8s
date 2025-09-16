/// Get configurable pathname variables without following symbolic link.
pub unsafe fn lpathconf<P: AsRef<Path>>(path: P, name: i32) -> Result<isize, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let name = name as usize;
    syscall2(SYS_LPATHCONF, path_ptr, name).map(|val| val as isize)
}
