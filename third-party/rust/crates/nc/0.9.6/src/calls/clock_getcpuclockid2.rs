/// Obtain ID of a process CPU-time clock.
pub unsafe fn clock_getcpuclockid2(
    id: id_t,
    which: i32,
    clock_id: &mut clockid_t,
) -> Result<(), Errno> {
    let id = id as usize;
    let which = which as usize;
    let clock_id_ptr = clock_id as *mut clockid_t as usize;
    syscall3(SYS_CLOCK_GETCPUCLOCKID2, id, which, clock_id_ptr).map(drop)
}
