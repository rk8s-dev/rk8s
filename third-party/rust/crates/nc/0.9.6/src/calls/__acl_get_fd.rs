pub unsafe fn __acl_get_fd(fd: i32, type_: acl_type_t, acl: &mut acl_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let type_ = type_ as usize;
    let acl_ptr = acl as *mut acl_t as usize;
    syscall3(SYS___ACL_GET_FD, fd, type_, acl_ptr).map(drop)
}
