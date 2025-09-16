/// Creates a processor set, and returns its ID into `psid`.
pub unsafe fn pset_create(psid: &mut psetid_t) -> Result<(), Errno> {
    let psid_ptr = psid as *mut psetid_t as usize;
    syscall1(SYS_PSET_CREATE, psid_ptr).map(drop)
}
