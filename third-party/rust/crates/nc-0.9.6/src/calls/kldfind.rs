/// Returns the fileid of a kld file.
pub unsafe fn kldfind<P: AsRef<Path>>(file: P) -> Result<i32, Errno> {
    let file = CString::new(file.as_ref());
    let file_ptr = file.as_ptr() as usize;
    syscall1(SYS_KLDFIND, file_ptr).map(|ret| ret as i32)
}
