/// Register files or user buffers for asynchronous I/O
pub unsafe fn io_uring_register(
    fd: u32,
    opcode: u32,
    arg: usize,
    nr_args: u32,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let opcode = opcode as usize;
    let nr_args = nr_args as usize;
    syscall4(SYS_IO_URING_REGISTER, fd, opcode, arg, nr_args).map(|ret| ret as i32)
}
