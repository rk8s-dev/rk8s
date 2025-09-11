/// Seals the VM's metadata from selected syscalls.
///
/// - addr/len: VM address range.
pub unsafe fn mseal(
    start: *const core::ffi::c_void,
    len: size_t,
    flags: usize,
) -> Result<(), Errno> {
    let start = start as usize;
    syscall3(SYS_MSEAL, start, len, flags).map(drop)
}
