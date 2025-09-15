/// Attempts to set the mask and policy for the object specified by `level`,
/// `which` and `id` to the values in `mask` and `policy`.
pub unsafe fn cpuset_setdomain(
    level: cpulevel_t,
    which: cpuwhich_t,
    id: id_t,
    domainset_size: size_t,
    mask: &domainset_t,
    policy: i32,
) -> Result<(), Errno> {
    let level = level as usize;
    let which = which as usize;
    let id = id as usize;
    let mask_ptr = mask as *const domainset_t as usize;
    let policy = policy as usize;
    syscall6(
        SYS_CPUSET_SETDOMAIN,
        level,
        which,
        id,
        domainset_size,
        mask_ptr,
        policy,
    )
    .map(drop)
}
