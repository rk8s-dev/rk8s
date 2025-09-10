/// Get static priority min value.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_get_priority_min(nc::SCHED_RR) };
/// assert!(ret.is_ok());
/// let min_prio = ret.unwrap();
/// assert_eq!(min_prio, 1);
/// ```
pub unsafe fn sched_get_priority_min(policy: i32) -> Result<i32, Errno> {
    let policy = policy as usize;
    syscall1(SYS_SCHED_GET_PRIORITY_MIN, policy).map(|ret| ret as i32)
}
