/// Attempts to set the mask for the object specified by `level`, `which` and `id`
/// to the `value` in mask.
pub unsafe fn cpuset_setaffinity(
    level: cpulevel_t,
    which: cpuwhich_t,
    id: id_t,
    mask: &[cpuset_t],
) -> Result<(), Errno> {
    let level = level as usize;
    let which = which as usize;
    let id = id as usize;
    let mask_len = mask.len();
    let mask_ptr = mask.as_ptr() as usize;
    syscall5(SYS_CPUSET_SETAFFINITY, level, which, id, mask_ptr, mask_len).map(drop)
}
