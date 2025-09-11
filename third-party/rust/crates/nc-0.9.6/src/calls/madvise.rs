/// Give advice about use of memory.
///
/// # Examples
///
/// ```
/// use std::ptr;
///
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let ret = unsafe {
///     nc::mmap(
///         ptr::null(),
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(ret.is_ok());
/// let addr = ret.unwrap();
///
/// // Notify kernel that the third page will be accessed.
/// let ret = unsafe { nc::madvise(addr.wrapping_add(2 * nc::PAGE_SIZE), nc::PAGE_SIZE, nc::MADV_WILLNEED) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn madvise(
    addr: *const core::ffi::c_void,
    len: size_t,
    advice: i32,
) -> Result<(), Errno> {
    let addr = addr as usize;
    let advice = advice as usize;
    syscall3(SYS_MADVISE, addr, len, advice).map(drop)
}
