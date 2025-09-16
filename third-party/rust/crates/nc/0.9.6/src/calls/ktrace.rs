/// Enables or disables tracing of one or more processes.
pub unsafe fn ktrace<P: AsRef<Path>>(
    tracefile: P,
    ops: i32,
    facs: i32,
    pid: i32,
) -> Result<(), Errno> {
    let tracefile = CString::new(tracefile.as_ref());
    let tracefile_ptr = tracefile.as_ptr() as usize;
    let ops = ops as usize;
    let facs = facs as usize;
    let pid = pid as usize;
    syscall4(SYS_KTRACE, tracefile_ptr, ops, facs, pid).map(drop)
}
