/// Retrieves the mask from the object specified by `level`, `which` and `id`
/// and stores it in the space provided by `mask`.
pub unsafe fn cpuset_getaffinity(
    level: cpulevel_t,
    which: cpuwhich_t,
    id: id_t,
    mask: &mut [cpuset_t],
) -> Result<(), Errno> {
    let level = level as usize;
    let which = which as usize;
    let id = id as usize;
    let mask_len = mask.len();
    let mask_ptr = mask.as_mut_ptr() as usize;
    syscall5(SYS_CPUSET_GETAFFINITY, level, which, id, mask_ptr, mask_len).map(drop)
}
