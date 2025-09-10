/// Remove an semaphore.
pub unsafe fn ksem_unlink<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS_KSEM_UNLINK, name_ptr).map(drop)
}
