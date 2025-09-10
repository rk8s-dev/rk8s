pub unsafe fn delete(path: user_addr_t) -> Result<(), Errno> {
    let path = path as usize;
    syscall1(SYS_DELETE, path).map(drop)
}
