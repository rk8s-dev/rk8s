pub unsafe fn vadvise(anom: i32) -> Result<i32, Errno> {
    let anom = anom as usize;
    syscall1(SYS_VADVISE, anom).map(|val| val as i32)
}
