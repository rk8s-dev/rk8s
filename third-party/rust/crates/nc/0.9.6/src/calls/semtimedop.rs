/// System V semaphore operations
pub unsafe fn semtimedop(
    semid: i32,
    sops: &mut [sembuf_t],
    timeout: Option<&timespec_t>,
) -> Result<(), Errno> {
    let semid = semid as usize;
    let sops_ptr = sops.as_mut_ptr() as usize;
    let nops = sops.len();
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    syscall4(SYS_SEMTIMEDOP, semid, sops_ptr, nops, timeout_ptr).map(drop)
}
