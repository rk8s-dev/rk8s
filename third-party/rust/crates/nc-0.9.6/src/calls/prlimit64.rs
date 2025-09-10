/// Get/set the resource limits of an arbitary process.
///
/// # Examples
///
/// ```
/// let mut old_limit = nc::rlimit64_t::default();
/// let ret = unsafe { nc::prlimit64(nc::getpid(), nc::RLIMIT_NOFILE, None, Some(&mut old_limit)) };
/// assert!(ret.is_ok());
/// assert!(old_limit.rlim_cur > 0);
/// assert!(old_limit.rlim_max > 0);
/// ```
pub unsafe fn prlimit64(
    pid: pid_t,
    resource: i32,
    new_limit: Option<&rlimit64_t>,
    old_limit: Option<&mut rlimit64_t>,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let resource = resource as usize;
    let new_limit_ptr = new_limit.map_or(core::ptr::null::<rlimit64_t>() as usize, |new_limit| {
        new_limit as *const rlimit64_t as usize
    });
    let old_limit_ptr = old_limit
        .map_or(core::ptr::null_mut::<rlimit64_t>() as usize, |old_limit| {
            old_limit as *mut rlimit64_t as usize
        });
    syscall4(SYS_PRLIMIT64, pid, resource, new_limit_ptr, old_limit_ptr).map(drop)
}
