pub unsafe fn open_tree<P: AsRef<Path>>(dfd: i32, filename: P, flags: u32) -> Result<i32, Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_OPEN_TREE, dfd, filename_ptr, flags).map(|ret| ret as i32)
}
