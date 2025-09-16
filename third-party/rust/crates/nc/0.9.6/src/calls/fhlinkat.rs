/// Make a hard link.
pub unsafe fn fhlinkat<P: AsRef<Path>>(fh: &mut fhandle_t, tofd: i32, to: P) -> Result<(), Errno> {
    let fh_ptr = fh as *mut fhandle_t as usize;
    let tofd = tofd as usize;
    let to = CString::new(to.as_ref());
    let to_ptr = to.as_ptr() as usize;
    syscall3(SYS_FHLINKAT, fh_ptr, tofd, to_ptr).map(drop)
}
