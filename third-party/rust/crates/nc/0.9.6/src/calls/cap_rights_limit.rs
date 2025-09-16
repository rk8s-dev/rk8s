/// Limit capability rights
pub unsafe fn cap_rights_limit(fd: i32, rights: &cap_rights_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let rights_ptr = rights as *const cap_rights_t as usize;
    syscall2(SYS_CAP_RIGHTS_LIMIT, fd, rights_ptr).map(drop)
}
