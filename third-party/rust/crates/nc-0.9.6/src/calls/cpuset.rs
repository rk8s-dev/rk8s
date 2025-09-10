/// Creates a new set containing the same CPUs as the root set of the current process
/// and stores its id in the space provided by `setid`.
pub unsafe fn cpuset(setid: &mut cpuset_t) -> Result<(), Errno> {
    let setid_ptr = setid as *mut cpuset_t as usize;
    syscall1(SYS_CPUSET, setid_ptr).map(drop)
}
