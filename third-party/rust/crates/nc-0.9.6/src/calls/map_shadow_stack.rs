pub unsafe fn map_shadow_stack(addr: usize, size: usize, flags: u32) -> Result<usize, Errno> {
    let flags = flags as usize;
    syscall3(SYS_MAP_SHADOW_STACK, addr, size, flags)
}
