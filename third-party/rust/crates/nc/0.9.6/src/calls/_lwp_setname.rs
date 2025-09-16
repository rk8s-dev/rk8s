pub unsafe fn _lwp_setname(target: lwpid_t, name: &str) -> Result<(), Errno> {
    let target = target as usize;
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS__LWP_SETNAME, target, name_ptr).map(drop)
}
