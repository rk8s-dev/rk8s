/// Retrieve audit session state
pub unsafe fn getaudit(info: &mut auditinfo_t) -> Result<(), Errno> {
    let info_ptr = info as *mut auditinfo_t as usize;
    syscall1(SYS_GETAUDIT, info_ptr).map(drop)
}
