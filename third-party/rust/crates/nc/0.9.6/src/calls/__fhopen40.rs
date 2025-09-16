/// Opens the file referenced by `fh` for reading and/or writing,
/// and returns the file descriptor to the calling process.
pub unsafe fn __fhopen40(fhp: uintptr_t, fh_size: size_t, flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall3(SYS___FHOPEN40, fhp, fh_size, flags).map(|val| val as i32)
}
