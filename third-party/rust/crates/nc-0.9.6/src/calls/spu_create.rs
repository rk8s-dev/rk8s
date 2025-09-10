/// Create a new spu context.
pub unsafe fn spu_create<P: AsRef<Path>>(
    name: P,
    flags: i32,
    mode: umode_t,
    neighbor_fd: i32,
) -> Result<i32, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    let neighbor_fd = neighbor_fd as usize;
    syscall4(SYS_SPU_CREATE, name_ptr, flags, mode, neighbor_fd).map(|ret| ret as i32)
}
