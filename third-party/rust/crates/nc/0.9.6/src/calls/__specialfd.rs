pub unsafe fn __specialfd(type_: i32, req: &[u8]) -> Result<i32, Errno> {
    let type_ = type_ as usize;
    let req_ptr = req.as_ptr() as usize;
    let req_len = req.len();
    // TODO(Shaohua): Check return type
    syscall3(SYS___SPECIALFD, type_, req_ptr, req_len).map(|val| val as i32)
}
