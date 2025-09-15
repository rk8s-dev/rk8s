/// Set the value of the VFS extended attribute specified, without following symlinks.
pub unsafe fn extattr_set_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
    data: &[u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_SET_LINK,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}
