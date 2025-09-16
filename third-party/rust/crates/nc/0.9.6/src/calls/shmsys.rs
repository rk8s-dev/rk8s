pub unsafe fn shmsys(which: i32, a2: i32, a3: i32, a4: i32) -> Result<(), Errno> {
    let which = which as usize;
    let a2 = a2 as usize;
    let a3 = a3 as usize;
    let a4 = a4 as usize;
    syscall4(SYS_SHMSYS, which, a2, a3, a4).map(drop)
}
