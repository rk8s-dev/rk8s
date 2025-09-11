/// Set file mode creation mask.
///
/// # Examples
///
/// ```
/// let new_mask = 0o077;
/// let ret = unsafe { nc::umask(new_mask) };
/// assert!(ret.is_ok());
/// let old_mask = ret.unwrap();
/// let ret = unsafe { nc::umask(old_mask) };
/// assert_eq!(ret, Ok(new_mask));
/// ```
pub unsafe fn umask(mode: mode_t) -> Result<mode_t, Errno> {
    let mode = mode as usize;
    syscall1(SYS_UMASK, mode).map(|ret| ret as mode_t)
}
