pub unsafe fn rtas(args: &mut rtas_args_t) -> Result<(), Errno> {
    let args_ptr = args as *mut rtas_args_t as usize;
    syscall1(SYS_RTAS, args_ptr).map(drop)
}
