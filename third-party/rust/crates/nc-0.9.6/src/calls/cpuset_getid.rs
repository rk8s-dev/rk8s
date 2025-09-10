/// Retrieves a set `id` from the object indicated by `which` and
/// stores it in the space pointed to by `setid`.
pub unsafe fn cpuset_getid(
    level: cpulevel_t,
    which: cpuwhich_t,
    id: id_t,
    setid: &mut cpuset_t,
) -> Result<(), Errno> {
    let level = level as usize;
    let which = which as usize;
    let id = id as usize;
    let setid_ptr = setid as *mut cpuset_t as usize;
    syscall4(SYS_CPUSET_GETID, level, which, id, setid_ptr).map(drop)
}
