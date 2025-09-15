/// Get login name.
pub unsafe fn getlogin(name: &mut [u8]) -> Result<(), Errno> {
    // TODO(Shaohua): Convert to CString
    let name_ptr = name.as_mut_ptr() as usize;
    let len = name.len();
    syscall2(SYS_GETLOGIN, name_ptr, len).map(drop)
}
