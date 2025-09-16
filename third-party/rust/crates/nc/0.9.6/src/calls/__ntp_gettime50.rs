/// Provides the time, maximum error (sync distance) and estimated error (dispersion)
/// to client user application programs.
pub unsafe fn __ntp_gettime50(time: &mut ntptimeval_t) -> Result<i32, Errno> {
    let time_ptr = time as *mut ntptimeval_t as usize;
    syscall1(SYS___NTP_GETTIME50, time_ptr).map(|val| val as i32)
}
