/// Sets up a jail and locks current process in it.
///
/// Returns jail identifier (JID).
pub unsafe fn jail(conf: &jail_t) -> Result<i32, Errno> {
    let conf_ptr = conf as *const jail_t as usize;
    syscall1(SYS_JAIL, conf_ptr).map(|ret| ret as i32)
}
