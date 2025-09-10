/// Control the inheritance of pages
pub unsafe fn minherit(addr: usize, len: size_t, inherit: i32) -> Result<(), Errno> {
    let inherit = inherit as usize;
    syscall3(SYS_MINHERIT, addr, len, inherit).map(drop)
}
