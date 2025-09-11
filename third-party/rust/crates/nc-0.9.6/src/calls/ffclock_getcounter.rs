/// Retrieve feed-forward counter.
pub unsafe fn ffclock_getcounter(ffcount: &mut ffcounter_t) -> Result<(), Errno> {
    let ffcount_ptr = ffcount as *mut ffcounter_t as usize;
    syscall1(SYS_FFCLOCK_GETCOUNTER, ffcount_ptr).map(drop)
}
