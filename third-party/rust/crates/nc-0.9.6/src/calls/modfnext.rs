/// Return the modid of the next kernel module
pub unsafe fn modfnext(modid: i32) -> Result<i32, Errno> {
    let modid = modid as usize;
    syscall1(SYS_MODFNEXT, modid).map(|ret| ret as i32)
}
