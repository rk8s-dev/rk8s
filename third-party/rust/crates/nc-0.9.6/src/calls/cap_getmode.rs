/// Returns a flag indicating whether or not the process is
/// in a capability mode sandbox.
pub unsafe fn cap_getmode(mode: &mut u32) -> Result<(), Errno> {
    let mode_ptr = mode as *mut u32 as usize;
    syscall1(SYS_CAP_GETMODE, mode_ptr).map(drop)
}
