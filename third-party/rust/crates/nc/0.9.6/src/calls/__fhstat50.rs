/// Get file status referenced by `fh`.
pub unsafe fn __fhstat50(fhp: uintptr_t, fh_size: size_t, sb: &mut stat_t) -> Result<(), Errno> {
    let sb_ptr = sb as *mut stat_t as usize;
    syscall3(SYS___FHSTAT50, fhp, fh_size, sb_ptr).map(drop)
}
