/// Set I/O scheduling class and priority.
///
/// See [ioprio](https://www.kernel.org/doc/Documentation/block/ioprio.txt)
///
/// # Examples
///
/// ```
/// // Change priority to lowest.
/// let new_prio_data = 7;
/// let new_prio = unsafe { nc::ioprio_prio_value(nc::IOPRIO_CLASS_IDLE, new_prio_data) };
/// let ret = unsafe { nc::ioprio_set(nc::IOPRIO_WHO_PROCESS, 0, new_prio) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ioprio_set(which: i32, who: i32, ioprio: i32) -> Result<(), Errno> {
    let which = which as usize;
    let who = who as usize;
    let ioprio = ioprio as usize;
    syscall3(SYS_IOPRIO_SET, which, who, ioprio).map(drop)
}
