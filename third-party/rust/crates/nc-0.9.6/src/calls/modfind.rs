/// Return the modid of a kernel module.
pub unsafe fn modfind(name: &str) -> Result<i32, Errno> {
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS_MODFIND, name_ptr).map(|ret| ret as i32)
}
