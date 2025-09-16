/// Initialize and open a named semaphore.
pub unsafe fn _ksem_open<P: AsRef<Path>>(
    name: P,
    flag: i32,
    mode: mode_t,
    value: u32,
    id: &mut intptr_t,
) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flag = flag as usize;
    let mode = mode as usize;
    let value = value as usize;
    let id_ptr = id as *mut intptr_t as usize;
    syscall5(SYS__KSEM_OPEN, name_ptr, flag, mode, value, id_ptr).map(drop)
}
