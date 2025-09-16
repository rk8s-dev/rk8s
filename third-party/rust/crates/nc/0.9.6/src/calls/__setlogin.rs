/// Set login name.
pub unsafe fn __setlogin(name: &str) -> Result<(), Errno> {
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS___SETLOGIN, name_ptr).map(drop)
}
