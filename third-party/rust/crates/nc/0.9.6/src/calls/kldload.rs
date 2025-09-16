/// Load kld files into the kernel.
pub unsafe fn kldload<P: AsRef<Path>>(file: P) -> Result<i32, Errno> {
    let file = CString::new(file.as_ref());
    let file_ptr = file.as_ptr() as usize;
    syscall1(SYS_KLDLOAD, file_ptr).map(|ret| ret as i32)
}
