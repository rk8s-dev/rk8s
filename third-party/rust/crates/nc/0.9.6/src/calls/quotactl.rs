/// Manipulate disk quotes.
pub unsafe fn quotactl<P: AsRef<Path>>(
    path: P,
    cmd: i32,
    id: i32,
    addr: usize,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let cmd = cmd as usize;
    let id = id as usize;
    syscall4(SYS_QUOTACTL, path_ptr, cmd, id, addr).map(drop)
}
