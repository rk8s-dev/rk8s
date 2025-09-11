/// Set memory policy for a memory range.
pub unsafe fn mbind(
    start: *const core::ffi::c_void,
    len: usize,
    mode: i32,
    nmask: &[usize],
    maxnode: usize,
    flags: u32,
) -> Result<(), Errno> {
    let start = start as usize;
    let mode = mode as usize;
    let nmask = nmask.as_ptr() as usize;
    let flags = flags as usize;
    syscall6(SYS_MBIND, start, len, mode, nmask, maxnode, flags).map(drop)
}
