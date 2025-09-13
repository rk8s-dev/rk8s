/// Removes the jail identified by `jid`.
///
/// It will kill all processes belonging to the jail, and
/// remove any children of that jail.
pub unsafe fn jail_remove(jid: i32) -> Result<(), Errno> {
    let jid = jid as usize;
    syscall1(SYS_JAIL_REMOVE, jid).map(drop)
}
