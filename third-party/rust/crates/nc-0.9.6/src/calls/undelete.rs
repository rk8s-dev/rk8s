/// Attempt to recover a deleted file
pub unsafe fn undelete<P: AsRef<Path>>(path: P) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall1(SYS_UNDELETE, path_ptr).map(drop)
}
