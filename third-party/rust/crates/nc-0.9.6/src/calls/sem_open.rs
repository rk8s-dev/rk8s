/// Creates or opens the named semaphore specified by `name`.
pub unsafe fn sem_open<P: AsRef<Path>>(
    name: P,
    flags: i32,
    mode: mode_t,
    value: u32,
) -> Result<user_addr_t, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    let value = value as usize;
    syscall4(SYS_SEM_OPEN, name_ptr, flags, mode, value).map(|ret| ret as user_addr_t)
}
