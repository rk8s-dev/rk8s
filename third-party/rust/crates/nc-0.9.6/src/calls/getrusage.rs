/// Get resource usage.
///
/// # Examples
///
/// ```
/// let mut usage = nc::rusage_t::default();
/// let ret = unsafe { nc::getrusage(nc::RUSAGE_SELF, &mut usage) };
/// assert!(ret.is_ok());
/// assert!(usage.ru_maxrss > 0);
/// assert_eq!(usage.ru_nswap, 0);
/// ```
pub unsafe fn getrusage(who: i32, usage: &mut rusage_t) -> Result<(), Errno> {
    let who = who as usize;
    let usage_ptr = usage as *mut rusage_t as usize;
    syscall2(SYS_GETRUSAGE, who, usage_ptr).map(drop)
}
