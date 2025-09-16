/// Get filesystem statistics referenced by `fh`.
pub unsafe fn __fhstatvfs140(
    fhp: uintptr_t,
    fh_size: size_t,
    buf: &mut statvfs_t,
    flags: i32,
) -> Result<(), Errno> {
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall4(SYS___FHSTATVFS140, fhp, fh_size, buf_ptr, flags).map(drop)
}
