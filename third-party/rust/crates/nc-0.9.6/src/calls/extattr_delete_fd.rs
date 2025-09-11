/// Deletes the VFS extended attribute specified.
pub unsafe fn extattr_delete_fd(
    fd: i32,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall3(SYS_EXTATTR_DELETE_FD, fd, attr_namespace, attr_name_ptr).map(drop)
}
