pub unsafe fn __acl_delete_fd(fd: i32, type_: acl_type_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let type_ = type_ as usize;
    syscall2(SYS___ACL_DELETE_FD, fd, type_).map(drop)
}
