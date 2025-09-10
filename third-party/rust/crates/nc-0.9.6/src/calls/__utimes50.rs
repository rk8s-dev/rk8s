/// Change file last access and modification time.
pub unsafe fn __utimes50<P: AsRef<Path>>(filename: P, times: &[timeval_t; 2]) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS___UTIMES50, filename_ptr, times_ptr).map(drop)
}
