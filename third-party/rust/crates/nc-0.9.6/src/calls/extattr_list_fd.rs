/// Returns a list of the VFS extended attributes present in the requested namespace.
pub unsafe fn extattr_list_fd(
    fd: i32,
    attr_namespace: i32,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall4(SYS_EXTATTR_LIST_FD, fd, attr_namespace, data_ptr, nbytes).map(|val| val as ssize_t)
}
