/// Creates a new kernel-scheduled thread of execution in the context of the current process.
pub unsafe fn thr_new(param: &mut thr_param_t) -> Result<(), Errno> {
    let param_ptr = param as *mut thr_param_t as usize;
    let param_size = core::mem::size_of::<thr_param_t>();
    syscall2(SYS_THR_NEW, param_ptr, param_size).map(drop)
}
