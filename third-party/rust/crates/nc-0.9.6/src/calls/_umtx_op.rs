/// Interface for implementation of userspace threading synchronization primitives
pub unsafe fn _umtx_op(
    obj: usize,
    op: i32,
    val: usize,
    addr: usize,
    addr2: usize,
) -> Result<i32, Errno> {
    let op = op as usize;
    syscall5(SYS__UMTX_OP, obj, op, val, addr, addr2).map(|val| val as i32)
}
