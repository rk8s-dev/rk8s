/// Copy a file.
pub unsafe fn copyfile<P: AsRef<Path>>(from: P, to: P, mode: i32, flags: u32) -> Result<(), Errno> {
    let from = CString::new(from.as_ref());
    let from_ptr = from.as_ptr() as usize;
    let to = CString::new(to.as_ref());
    let to_ptr = to.as_ptr() as usize;
    let mode = mode as usize;
    let flags = flags as usize;
    syscall4(SYS_COPYFILE, from_ptr, to_ptr, mode, flags).map(drop)
}
