/// System call vectors.
///
/// Argument checking cleaned up. Saved 20% in size.
/// This function doesn't need to set the kernel lock because
/// it is set by the callees.
// TODO(Shaohua): Check args type and return type
pub unsafe fn socketcall(call: i32, args: &mut usize) -> Result<usize, Errno> {
    let call = call as usize;
    let args_ptr = args as *mut usize as usize;
    syscall2(SYS_SOCKETCALL, call, args_ptr)
}
