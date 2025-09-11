/// Execute an SPU context.
pub unsafe fn spu_run(fd: i32, npc: &mut u32, status: &mut u32) -> Result<usize, Errno> {
    let fd = fd as usize;
    let npc_ptr = npc as *mut u32 as usize;
    let status_ptr = status as *mut u32 as usize;
    syscall3(SYS_SPU_RUN, fd, npc_ptr, status_ptr)
}
