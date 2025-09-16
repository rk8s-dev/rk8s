/// Set audit session ID
pub unsafe fn setauid(auid: &mut au_id_t) -> Result<(), Errno> {
    let auid_ptr = auid as *mut au_id_t as usize;
    syscall1(SYS_SETAUID, auid_ptr).map(drop)
}
