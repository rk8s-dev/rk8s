/// Module control.
pub unsafe fn modctl(operation: i32, arg: uintptr_t) -> Result<(), Errno> {
    let operation = operation as usize;
    syscall2(SYS_MODCTL, operation, arg).map(drop)
}
