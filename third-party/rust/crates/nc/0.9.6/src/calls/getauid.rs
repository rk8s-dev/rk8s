/// Retrieve audit session ID
pub unsafe fn getauid(auid: &mut au_id_t) -> Result<(), Errno> {
    let auid_ptr = auid as *mut au_id_t as usize;
    syscall1(SYS_GETAUID, auid_ptr).map(drop)
}
