/// Update visible name of specific thread.
pub unsafe fn thr_set_name(id: isize, name: &str) -> Result<(), Errno> {
    let id = id as usize;
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS_THR_SET_NAME, id, name_ptr).map(drop)
}
