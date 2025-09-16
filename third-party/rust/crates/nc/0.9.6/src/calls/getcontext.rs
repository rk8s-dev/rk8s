/// Get user thread context.
pub unsafe fn getcontext(ctx: &mut ucontext_t) -> Result<(), Errno> {
    let ctx_ptr = ctx as *mut ucontext_t as usize;
    syscall1(SYS_GETCONTEXT, ctx_ptr).map(drop)
}
