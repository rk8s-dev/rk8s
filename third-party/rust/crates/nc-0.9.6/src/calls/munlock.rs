/// Unlock memory.
///
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let addr = passwd_buf.as_ptr() as *const _;
/// let ret = unsafe { nc::mlock2(addr, passwd_buf.len(), nc::MCL_CURRENT) };
/// for i in 0..passwd_buf.len() {
///   passwd_buf[i] = i as u8;
/// }
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::munlock(addr, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munlock(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MUNLOCK, addr, len).map(drop)
}
