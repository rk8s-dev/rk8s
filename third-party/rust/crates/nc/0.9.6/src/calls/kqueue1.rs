/// Creates a new kernel event queue and returns a descriptor.
pub unsafe fn kqueue1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_KQUEUE1, flags).map(|val| val as i32)
}
