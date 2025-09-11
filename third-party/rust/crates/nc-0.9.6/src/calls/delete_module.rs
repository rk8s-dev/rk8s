/// Unlock a kernel module.
pub unsafe fn delete_module<P: AsRef<Path>>(name: P, flags: u32) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_DELETE_MODULE, name_ptr, flags).map(drop)
}
