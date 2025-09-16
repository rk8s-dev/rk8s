/// Load a new kernel for later execution.
pub unsafe fn kexec_load(
    entry: usize,
    segments: &mut [kexec_segment_t],
    flags: u32,
) -> Result<(), Errno> {
    let segments_ptr = segments.as_mut_ptr() as usize;
    let nr_segments = segments.len();
    let flags = flags as usize;
    syscall4(SYS_KEXEC_LOAD, entry, nr_segments, segments_ptr, flags).map(drop)
}
