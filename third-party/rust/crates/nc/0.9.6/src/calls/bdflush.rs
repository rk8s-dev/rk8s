/// Start, flush or tune buffer-dirty-flush daemon.
/// There are no bdflush tunables left.  But distributions are
/// still running obsolete flush daemons, so we terminate them here.
///
/// Use of `bdflush()` is deprecated and will be removed in a future kernel.
/// The `flush-X` kernel threads fully replace bdflush daemons and this call.
/// Deprecated.
pub unsafe fn bdflush() {
    core::unimplemented!();
    // syscall0(SYS_BDFLUSH);
}
