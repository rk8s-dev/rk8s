pub unsafe fn __acl_set_link<P: AsRef<Path>>(
    path: P,
    type_: acl_type_t,
    acl: &mut acl_t,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let type_ = type_ as usize;
    let acl_ptr = acl as *mut acl_t as usize;
    syscall3(SYS___ACL_SET_LINK, path_ptr, type_, acl_ptr).map(drop)
}
