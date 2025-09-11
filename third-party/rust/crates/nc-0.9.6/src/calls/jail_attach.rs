/// Attaches the current process to an existing jail.
pub unsafe fn jail_attach(jid: i32) -> Result<(), Errno> {
    let jid = jid as usize;
    syscall1(SYS_JAIL_ATTACH, jid).map(drop)
}
