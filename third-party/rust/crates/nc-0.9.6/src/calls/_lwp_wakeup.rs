pub unsafe fn _lwp_wakeup(target: lwpid_t) -> Result<(), Errno> {
    let target = target as usize;
    syscall1(SYS__LWP_WAKEUP, target).map(drop)
}
