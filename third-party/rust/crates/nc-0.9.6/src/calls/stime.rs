/// Set time.
///
/// # Examples
///
/// ```
/// let t = 1611630530;
/// let ret = unsafe { nc::stime(&t) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn stime(t: &time_t) -> Result<(), Errno> {
    let t_ptr = t as *const time_t as usize;
    syscall1(SYS_STIME, t_ptr).map(drop)
}
