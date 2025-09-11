/// Get file status about a file, without following symbolic.
pub unsafe fn __lstat50<P: AsRef<Path>>(filename: P, statbuf: &mut stat_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS___LSTAT50, filename_ptr, statbuf_ptr).map(drop)
}
