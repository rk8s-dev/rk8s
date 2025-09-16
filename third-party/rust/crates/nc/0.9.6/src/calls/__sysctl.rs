/// Read/write system parameters.
pub unsafe fn __sysctl(
    name: &i32,
    name_len: u32,
    old_val: usize,
    old_len: size_t,
    new_val: usize,
    new_len: size_t,
) -> Result<(), Errno> {
    let name_ptr = name as *const i32 as usize;
    let name_len = name_len as usize;
    syscall6(
        SYS___SYSCTL,
        name_ptr,
        name_len,
        old_val,
        old_len,
        new_val,
        new_len,
    )
    .map(drop)
}
