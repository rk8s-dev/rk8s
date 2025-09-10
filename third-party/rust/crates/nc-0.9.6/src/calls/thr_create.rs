/// Crate a new thread.
pub unsafe fn thr_craete(ctx: &mut ucontext_t, id: &mut isize, flags: i32) -> Result<(), Errno> {
    let ctx_ptr = ctx as *mut ucontext_t as usize;
    let id_ptr = id as *mut isize as usize;
    let flags = flags as usize;
    syscall3(SYS_THR_CREATE, ctx_ptr, id_ptr, flags).map(drop)
}
