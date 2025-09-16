/// Set audit session state
pub unsafe fn setaudit(info: &mut auditinfo_t) -> Result<(), Errno> {
    let info_ptr = info as *mut auditinfo_t as usize;
    syscall1(SYS_SETAUDIT, info_ptr).map(drop)
}
