/// Unmap files or devices from memory.
///
/// # Examples
///
/// ```
/// use std::{mem, ptr};
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let mut sb = nc::stat_t::default();
/// let ret = unsafe { nc::fstat(fd, &mut sb) };
/// assert!(ret.is_ok());
///
/// let offset: usize = 0;
/// let length: usize = sb.st_size as usize - offset;
/// // Offset for mmap must be page aligned.
/// let pa_offset: usize = offset & !(nc::PAGE_SIZE - 1);
/// let map_length = length + offset - pa_offset;
///
/// let addr = unsafe {
///     nc::mmap(
///         ptr::null(),
///         map_length,
///         nc::PROT_READ,
///         nc::MAP_PRIVATE,
///         fd,
///         pa_offset as nc::off_t,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
/// let stdout = 1;
///
/// // Create the "fat pointer".
/// let buf = unsafe {
///     mem::transmute::<(usize, usize), &[u8]>((addr as usize + offset - pa_offset, length))
/// };
/// let n_write = unsafe { nc::write(stdout, buf) };
/// assert!(n_write.is_ok());
/// assert_eq!(n_write, Ok(length as nc::ssize_t));
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munmap(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MUNMAP, addr, len).map(drop)
}
