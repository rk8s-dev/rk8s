/// Deletes the VFS extended attribute specified.
pub unsafe fn extattr_delete_file<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall3(
        SYS_EXTATTR_DELETE_FILE,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
    )
    .map(drop)
}
