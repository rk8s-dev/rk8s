/// Get list of all mounted file systems.
///
/// If buf is None, returns number of mounted file systems.
pub unsafe fn getfsstat(buf: Option<&mut [statfs_t]>, mode: i32) -> Result<i32, Errno> {
    let buf_size = buf
        .as_ref()
        .map_or(0, |buf| buf.len() * core::mem::size_of::<statfs_t>());
    let buf_ptr = buf.map_or(core::mem::null_mut::<statfs_t>() as usize, |buf| {
        buf.as_mut_ptr() as usize
    });
    let mode = mode as usize;
    syscall3(SYS_GETFSSTAT, buf_ptr, buf_size, mode).map(|val| val as i32)
}
