pub unsafe fn __acl_delete_file<P: AsRef<Path>>(path: P, type_: acl_type_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let type_ = type_ as usize;
    syscall2(SYS___ACL_DELETE_FILE, path_ptr, type_).map(drop)
}
