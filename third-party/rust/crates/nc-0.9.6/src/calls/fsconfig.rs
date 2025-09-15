/// Set parameters and trigger actions on a context.
pub unsafe fn fsconfig<P: AsRef<Path>>(
    fd: i32,
    cmd: u32,
    key: P,
    value: P,
    aux: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let key = CString::new(key.as_ref());
    let key_ptr = key.as_ptr() as usize;
    let value = CString::new(value.as_ref());
    let value_ptr = value.as_ptr() as usize;
    let aux = aux as usize;
    syscall5(SYS_FSCONFIG, fd, cmd, key_ptr, value_ptr, aux).map(drop)
}
