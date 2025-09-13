/// Set protection on a region of memory.
///
/// # Examples
///
/// ```
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let addr = unsafe {
///     nc::mmap(
///         std::ptr::null(),
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
///
/// // Set the third page readonly. And we will run into SIGSEGV when updating it.
/// let ret = unsafe { nc::mprotect(addr.wrapping_add(2 * nc::PAGE_SIZE), nc::PAGE_SIZE, nc::PROT_READ) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mprotect(
    addr: *const core::ffi::c_void,
    len: size_t,
    prot: i32,
) -> Result<(), Errno> {
    let addr = addr as usize;
    let prot = prot as usize;
    syscall3(SYS_MPROTECT, addr, len, prot).map(drop)
}
