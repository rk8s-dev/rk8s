/// Set hostname.
///
/// # Exampless
///
/// ```
/// let name = "rust-machine";
/// let ret = unsafe { nc::sethostname(name) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn sethostname<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let name_len = name.len();
    syscall2(SYS_SETHOSTNAME, name_ptr, name_len).map(drop)
}
