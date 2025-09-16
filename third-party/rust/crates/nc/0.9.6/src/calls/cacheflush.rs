/// Flush contents of instruction and/or data cache.
pub unsafe fn cacheflush(addr: usize, nbytes: size_t, cache: i32) -> Result<(), Errno> {
    let nbytes = nbytes as usize;
    let cache = cache as usize;
    syscall3(SYS_CACHEFLUSH, addr, nbytes, cache).map(drop)
}
