/// Manipulate the kernel's key management facility.
pub unsafe fn keyctl(
    operation: i32,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> Result<usize, Errno> {
    let operation = operation as usize;
    syscall5(SYS_KEYCTL, operation, arg2, arg3, arg4, arg5)
}
