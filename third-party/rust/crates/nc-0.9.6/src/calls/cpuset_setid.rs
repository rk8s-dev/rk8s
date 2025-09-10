/// Attempts to set the `id` of the object specified by the `which` argument.
pub unsafe fn cpuset_setid(which: cpuwhich_t, id: id_t, setid: &cpuset_t) -> Result<(), Errno> {
    let which = which as usize;
    let id = id as usize;
    let setid = setid as *const cpuset_t as usize;
    syscall3(SYS_CPUSET_SETID, which, id, setid).map(drop)
}
