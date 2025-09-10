/// Creates a processor set, and returns its ID into `psid`.
pub unsafe fn pset_assign(
    psid: psetid_t,
    cpuid: cpuid_t,
    old_psid: Option<&mut psetid_t>,
) -> Result<(), Errno> {
    let psid = psid as usize;
    let cpuid = cpuid as usize;
    let old_psid_ptr = old_psid.map_or(0, |old_psid| old_psid as *mut psetid_t as usize);
    syscall3(SYS_PSET_ASSIGN, psid, cpuid, old_psid_ptr).map(drop)
}
