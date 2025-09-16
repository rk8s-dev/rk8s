/// Get filesystem statistics referenced by `fh`.
pub unsafe fn fhstatfs(fh: &fhandle_t, buf: &mut statfs_t) -> Result<(), Errno> {
    let fh_ptr = fh as *const fhandle_t as usize;
    let buf_ptr = buf as *mut statfs_t as usize;
    syscall2(SYS_FHSTATFS, fh_ptr, buf_ptr).map(drop)
}
