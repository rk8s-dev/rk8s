/// Return the modid of the next kernel module
pub unsafe fn modnext(modid: i32) -> Result<i32, Errno> {
    let modid = modid as usize;
    syscall1(SYS_MODNEXT, modid).map(|ret| ret as i32)
}
