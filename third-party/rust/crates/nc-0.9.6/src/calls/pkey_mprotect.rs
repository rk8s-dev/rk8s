/// Set protection on a region of memory.
pub unsafe fn pkey_mprotect(
    start: *const core::ffi::c_void,
    len: size_t,
    prot: i32,
    pkey: i32,
) -> Result<(), Errno> {
    let start = start as usize;
    let prot = prot as usize;
    let pkey = pkey as usize;
    syscall4(SYS_PKEY_MPROTECT, start, len, prot, pkey).map(drop)
}
