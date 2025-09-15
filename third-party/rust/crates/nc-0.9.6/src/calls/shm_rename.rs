/// Atomically removes a shared memory object named `path_from` and
/// relinks it at `path_to`.
pub unsafe fn shm_rename<P: AsRef<Path>>(
    path_from: P,
    path_to: P,
    flags: i32,
) -> Result<(), Errno> {
    let path_from = CString::new(path_from.as_ref());
    let path_from_ptr = path_from.as_ptr() as usize;
    let path_to = CString::new(path_to.as_ref());
    let path_to_ptr = path_to.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_SHM_RENAME, path_from_ptr, path_to_ptr, flags).map(drop)
}
