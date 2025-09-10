/// Set a thread's CPU affinity mask.
///
/// # Examples
///
/// ```
/// let mut set = nc::cpu_set_t::default();
/// assert!(set.set(1).is_ok());
/// println!("set(1): {:?}", set.is_set(1));
/// assert!(set.set(2).is_ok());
/// assert!(set.clear(2).is_ok());
/// println!("set(2): {:?}", set.is_set(2));
///
/// let ret = unsafe { nc::sched_setaffinity(0, &set) };
/// assert!(ret.is_ok());
///
/// let mut set2 = nc::cpu_set_t::default();
/// let ret = unsafe { nc::sched_getaffinity(0, &mut set2) };
/// assert!(ret.is_ok());
/// assert_eq!(set, set2);
/// ```
pub unsafe fn sched_setaffinity(pid: pid_t, user_mask: &cpu_set_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let cpu_set_len = core::mem::size_of::<cpu_set_t>();
    let user_mask_ptr = user_mask as *const cpu_set_t as usize;
    syscall3(SYS_SCHED_SETAFFINITY, pid, cpu_set_len, user_mask_ptr).map(drop)
}
