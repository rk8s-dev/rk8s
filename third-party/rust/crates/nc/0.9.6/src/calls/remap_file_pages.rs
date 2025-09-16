/// Create a nonlinear file mapping.
///
/// Deprecated.
pub unsafe fn remap_file_pages(
    start: *const core::ffi::c_void,
    size: size_t,
    prot: i32,
    pgoff: off_t,
    flags: i32,
) -> Result<(), Errno> {
    let start = start as usize;
    let prot = prot as usize;
    let pgoff = pgoff as usize;
    // NOTE(Shaohua): Type of flags is usize in kernel.
    let flags = flags as usize;
    syscall5(SYS_REMAP_FILE_PAGES, start, size, prot, pgoff, flags).map(drop)
}
