/// Make a hard link.
pub unsafe fn fhlink<P: AsRef<Path>>(fh: &mut fhandle_t, to: P) -> Result<(), Errno> {
    let fh_ptr = fh as *mut fhandle_t as usize;
    let to = CString::new(to.as_ref());
    let to_ptr = to.as_ptr() as usize;
    syscall2(SYS_FHLINK, fh_ptr, to_ptr).map(drop)
}
