/// Get list of supplementary group Ids.
///
/// # Examples
///
/// ```
/// let mut groups = vec![];
/// let ret = unsafe { nc::getgroups(&mut groups) };
/// assert!(ret.is_ok());
/// let total_num = ret.unwrap();
/// groups.resize(total_num as usize, 0);
///
/// let ret = unsafe { nc::getgroups(&mut groups) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(total_num));
/// ```
pub unsafe fn getgroups(group_list: &mut [gid_t]) -> Result<i32, Errno> {
    let size = group_list.len();
    let group_ptr = group_list.as_mut_ptr() as usize;
    syscall2(SYS_GETGROUPS, size, group_ptr).map(|ret| ret as i32)
}
