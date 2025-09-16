/// Change permissions of a file.
pub unsafe fn chmod_extended<P: AsRef<Path>>(
    path: P,
    uid: uid_t,
    gid: gid_t,
    mode: i32,
    xsecurity: user_addr_t,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let uid = uid as usize;
    let gid = gid as usize;
    let mode = mode as usize;
    let xsecurity = xsecurity as usize;
    syscall5(SYS_CHMOD_EXTENDED, path_ptr, uid, gid, mode, xsecurity).map(drop)
}
