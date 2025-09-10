/// Create and initialize fanotify group.
pub unsafe fn fanotify_init(flags: u32, event_f_flags: u32) -> Result<i32, Errno> {
    let flags = flags as usize;
    let event_f_flags = event_f_flags as usize;
    syscall2(SYS_FANOTIFY_INIT, flags, event_f_flags).map(|ret| ret as i32)
}
