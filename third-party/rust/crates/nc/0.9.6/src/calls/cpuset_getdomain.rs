/// Retrieves the mask and policy from the object specified by `level`,
/// `which` and `id` and stores it in the space provided by `mask` and `policy`.
pub unsafe fn cpuset_getdomain(
    level: cpulevel_t,
    which: cpuwhich_t,
    id: id_t,
    domainset_size: size_t,
    mask: &mut domainset_t,
    policy: &mut i32,
) -> Result<(), Errno> {
    let level = level as usize;
    let which = which as usize;
    let id = id as usize;
    let mask_ptr = mask as *mut domainset_t as usize;
    let policy_ptr = policy as *mut i32 as usize;
    syscall6(
        SYS_CPUSET_GETDOMAIN,
        level,
        which,
        id,
        domainset_size,
        mask_ptr,
        policy_ptr,
    )
    .map(drop)
}
