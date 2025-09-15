/// Manage UFS1 extended attributes
pub unsafe fn extattrctl<P: AsRef<Path>>(
    path: P,
    cmd: i32,
    filename: P,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let cmd = cmd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name = CString::new(attr_name);
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall5(
        SYS_EXTATTRCTL,
        path_ptr,
        cmd,
        filename_ptr,
        attr_namespace,
        attr_name_ptr,
    )
    .map(drop)
}
