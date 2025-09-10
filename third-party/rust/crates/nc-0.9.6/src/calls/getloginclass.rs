/// Get login class.
pub unsafe fn getloginclass(name: &mut [u8]) -> Result<(), Errno> {
    // TODO(Shaohua): Convert to CString
    let name_ptr = name.as_mut_ptr() as usize;
    let len = name.len();
    syscall2(SYS_GETLOGINCLASS, name_ptr, len).map(drop)
}
