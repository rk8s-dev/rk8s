/// System V IPC system calls.
pub unsafe fn ipc(
    call: u32,
    first: i32,
    second: i32,
    third: i32,
    ptr: usize,
    fifth: isize,
) -> Result<(), Errno> {
    let call = call as usize;
    let first = first as usize;
    let second = second as usize;
    let third = third as usize;
    let fifth = fifth as usize;
    syscall6(SYS_IPC, call, first, second, third, ptr, fifth).map(drop)
}
