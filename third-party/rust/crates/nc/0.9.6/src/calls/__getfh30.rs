/// Get file handle.
pub unsafe fn __getfh30<P: AsRef<Path>>(
    path: P,
    fhp: uintptr_t,
    fh_size: size_t,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall3(SYS___GETFH30, path_ptr, fhp, fh_size).map(drop)
}
