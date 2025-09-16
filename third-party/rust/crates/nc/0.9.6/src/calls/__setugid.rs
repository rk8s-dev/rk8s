pub unsafe fn __setugid(flag: i32) -> Result<(), Errno> {
    let flag = flag as usize;
    // TODO(Shaohua): Check return type
    syscall1(SYS___SETUGID, flag).map(drop)
}
