/// Remap a virtual memory address
///
/// # Examples
///
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
///
/// // Initialize an anonymous mapping with 2 pages.
/// let map_length = 2 * nc::PAGE_SIZE;
///
/// let addr = unsafe {
///     nc::mmap(
///         ptr::null(),
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr: *const c_void = addr.unwrap();
///
/// let new_map_length = 4 * nc::PAGE_SIZE;
/// let new_addr = unsafe {
///     nc::mremap(
///         addr,
///         map_length,
///         new_map_length,
///         nc::MREMAP_MAYMOVE,
///         ptr::null(),
///     )
/// };
/// if let Err(errno) = new_addr {
///     eprintln!("mremap() err: {}", nc::strerror(errno));
/// }
/// assert!(new_addr.is_ok());
/// let new_addr: *const c_void = new_addr.unwrap();
///
/// let ret = unsafe { nc::munmap(new_addr, map_length) };
/// assert!(ret.is_ok());
/// unsafe { nc::exit(0) };
/// ```
pub unsafe fn mremap(
    addr: *const core::ffi::c_void,
    old_len: size_t,
    new_len: size_t,
    flags: i32,
    new_addr: *const core::ffi::c_void,
) -> Result<*const core::ffi::c_void, Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    let new_addr = new_addr as usize;
    syscall5(SYS_MREMAP, addr, old_len, new_len, flags, new_addr)
        .map(|ret| ret as *const core::ffi::c_void)
}
