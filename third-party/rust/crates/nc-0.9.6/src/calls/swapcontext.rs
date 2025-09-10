/// Exchange user thread context.
pub unsafe fn swapcontext(old_ctx: &mut ucontext_t, ctx: &ucontext_t) -> Result<(), Errno> {
    let old_ctx_ptr = old_ctx as *mut ucontext_t as usize;
    let ctx_ptr = ctx as *const ucontext_t as usize;
    syscall2(SYS_SWAPCONTEXT, old_ctx_ptr, ctx_ptr).map(drop)
}
