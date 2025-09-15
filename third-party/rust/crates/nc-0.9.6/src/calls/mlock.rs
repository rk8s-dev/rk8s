/// Lock memory.
///
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let ret = unsafe { nc::mlock(passwd_buf.as_ptr() as *const _, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mlock(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MLOCK, addr, len).map(drop)
}
