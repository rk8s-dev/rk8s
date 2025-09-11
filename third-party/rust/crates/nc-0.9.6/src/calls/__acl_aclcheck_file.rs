pub unsafe fn __acl_aclcheck_file<P: AsRef<Path>>(
    path: P,
    type_: acl_type_t,
    acl: &mut acl_t,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let type_ = type_ as usize;
    let acl_ptr = acl as *mut acl_t as usize;
    syscall3(SYS___ACL_ACLCHECK_FILE, path_ptr, type_, acl_ptr).map(drop)
}
