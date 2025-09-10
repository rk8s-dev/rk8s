/// Returns a list of the VFS extended attributes present in the requested namespace,
/// without following symlinks.
pub unsafe fn extattr_list_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall4(
        SYS_EXTATTR_LIST_LINK,
        path_ptr,
        attr_namespace,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}
