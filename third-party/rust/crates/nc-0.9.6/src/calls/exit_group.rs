/// Exit all threads in a process's thread group.
///
/// # Examples
///
/// ```
/// unsafe { nc::exit_group(0); }
/// ```
pub unsafe fn exit_group(status: i32) -> ! {
    let status = status as usize;
    let _ret = syscall1(SYS_EXIT_GROUP, status);
    unreachable!();
}
