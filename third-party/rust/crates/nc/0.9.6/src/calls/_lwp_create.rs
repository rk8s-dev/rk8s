/// Crate a new thread.
pub unsafe fn _lwp_craete(
    ctx: &mut ucontext_t,
    flags: usize,
    id: &mut lwpid_t,
) -> Result<(), Errno> {
    let ctx_ptr = ctx as *mut ucontext_t as usize;
    let id_ptr = id as *mut lwpid_t as usize;
    syscall3(SYS__LWP_CREATE, ctx_ptr, flags, id_ptr).map(drop)
}
