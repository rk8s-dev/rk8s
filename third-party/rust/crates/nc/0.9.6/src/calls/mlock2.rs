/// Lock memory.
///
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let ret = unsafe { nc::mlock2(passwd_buf.as_ptr() as *const _, passwd_buf.len(), nc::MCL_CURRENT) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mlock2(addr: *const core::ffi::c_void, len: size_t, flags: i32) -> Result<(), Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    syscall3(SYS_MLOCK2, addr, len, flags).map(drop)
}
