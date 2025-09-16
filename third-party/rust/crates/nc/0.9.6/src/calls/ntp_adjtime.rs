/// Used by the NTP daemon to adjust the system clock to an externally derived time.
pub unsafe fn ntp_adjtime(time: &mut timex_t) -> Result<i32, Errno> {
    let time_ptr = time as *mut timex_t as usize;
    syscall1(SYS_NTP_ADJTIME, time_ptr).map(|val| val as i32)
}
