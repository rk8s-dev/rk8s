/// Remove an semaphore.
pub unsafe fn sem_unlink<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS_SEM_UNLINK, name_ptr).map(drop)
}
