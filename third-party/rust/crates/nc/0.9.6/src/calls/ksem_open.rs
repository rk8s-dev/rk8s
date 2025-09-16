/// Creates or opens the named semaphore specified by `name`.
pub unsafe fn ksem_open<P: AsRef<Path>>(
    name: P,
    flags: i32,
    mode: mode_t,
    value: u32,
    id: &mut semid_t,
) -> Result<i32, Errno> {
    // TODO(Shaohua): Replace with CStr
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    let value = value as usize;
    let id_ptr = id as *mut semid_t as usize;
    syscall5(SYS_KSEM_OPEN, name_ptr, flags, mode, value, id_ptr).map(|ret| ret as i32)
}
