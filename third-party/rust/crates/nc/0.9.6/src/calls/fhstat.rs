/// Get file status referenced by `fh`.
pub unsafe fn fhstat(fh: &fhandle_t, sb: &mut stat_t) -> Result<(), Errno> {
    let fh_ptr = fh as *const fhandle_t as usize;
    let sb_ptr = sb as *mut stat_t as usize;
    syscall2(SYS_FHSTAT, fh_ptr, sb_ptr).map(drop)
}
