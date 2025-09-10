/// Pick a superblock into a context for reconfiguration.
pub unsafe fn fspick<P: AsRef<Path>>(dfd: i32, path: P, flags: u32) -> Result<i32, Errno> {
    let dfd = dfd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_FSPICK, dfd, path_ptr, flags).map(|ret| ret as i32)
}
