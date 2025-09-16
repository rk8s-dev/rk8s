/// Revoke file access
pub unsafe fn revoke<P: AsRef<Path>>(path: P) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall1(SYS_REVOKE, path_ptr).map(drop)
}
