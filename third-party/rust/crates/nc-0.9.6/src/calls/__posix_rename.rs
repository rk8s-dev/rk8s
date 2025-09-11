/// Change name or location of a file.
pub unsafe fn __posix_rename<P: AsRef<Path>>(oldfilename: P, newfilename: P) -> Result<(), Errno> {
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    syscall2(SYS___POSIX_RENAME, oldfilename_ptr, newfilename_ptr).map(drop)
}
