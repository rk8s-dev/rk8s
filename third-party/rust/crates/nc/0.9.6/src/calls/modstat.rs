/// Get status of a kernel module.
pub unsafe fn modstat(modid: i32, stat: &mut module_stat_t) -> Result<(), Errno> {
    let modid = modid as usize;
    let stat_ptr = stat as *mut module_stat_t as usize;
    syscall2(SYS_MODSTAT, modid, stat_ptr).map(drop)
}
