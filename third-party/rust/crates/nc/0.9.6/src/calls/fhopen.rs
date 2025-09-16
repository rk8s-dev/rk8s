/// Opens the file referenced by `fh` for reading and/or writing,
/// and returns the file descriptor to the calling process.
pub unsafe fn fhopen(fh: &fhandle_t, flags: i32) -> Result<i32, Errno> {
    let fh_ptr = fh as *const fhandle_t as usize;
    let flags = flags as usize;
    syscall2(SYS_FHOPEN, fh_ptr, flags).map(|val| val as i32)
}
