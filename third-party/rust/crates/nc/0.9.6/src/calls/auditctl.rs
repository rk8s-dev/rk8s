/// Configure system audit parameters
pub unsafe fn auditctl<P: AsRef<Path>>(path: P) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall1(SYS_AUDITCTL, path_ptr).map(drop)
}
