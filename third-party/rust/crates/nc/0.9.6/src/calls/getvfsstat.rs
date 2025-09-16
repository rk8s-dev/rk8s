/// Get list of all mounted file systems.
///
/// If buf is None, returns number of mounted file systems.
pub unsafe fn getvfsstat(buf: Option<&mut [statvfs_t]>, mode: i32) -> Result<i32, Errno> {
    let buf_size = buf
        .as_ref()
        .map_or(0, |buf| buf.len() * core::mem::size_of::<statvfs_t>());
    let buf_ptr = buf.map_or(core::mem::null_mut::<statvfs_t>() as usize, |buf| {
        buf.as_mut_ptr() as usize
    });
    let mode = mode as usize;
    syscall3(SYS_GETVFSSTAT, buf_ptr, buf_size, mode).map(|val| val as i32)
}
