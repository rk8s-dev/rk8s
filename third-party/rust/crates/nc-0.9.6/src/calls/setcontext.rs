/// Set user thread context.
pub unsafe fn setcontext(ctx: &ucontext_t) -> Result<(), Errno> {
    let ctx_ptr = ctx as *const ucontext_t as usize;
    syscall1(SYS_SETCONTEXT, ctx_ptr).map(drop)
}
