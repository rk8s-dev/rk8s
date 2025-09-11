/// Fast user-space locking.
///
/// Parameters
/// - `uaddr`: futex user address
/// - `op`: futex operations
/// - `val`: expected value
/// - `utime`: waiting timeout
/// - `uaddr2`: target futext user address used for requeue
///
/// # Exampless
///
/// ```rust
/// use std::sync::atomic::{AtomicU32, Ordering};
/// use std::thread;
/// use std::time::Duration;
///
/// const NOTIFY_WAIT: u32 = 0;
/// const NOTIFY_WAKE: u32 = 1;
///
/// fn wake_one(count: &AtomicU32) {
///     let ret = unsafe { nc::futex(count, nc::FUTEX_WAKE, NOTIFY_WAKE, None, None, 0) };
///     assert!(ret.is_ok());
/// }
///
/// fn wait(count: &AtomicU32, expected: u32) {
///     let ret = unsafe { nc::futex(count, nc::FUTEX_WAIT, expected, None, None, 0) };
///     assert!(ret.is_ok());
/// }
///
/// fn main() {
///     let notify = AtomicU32::new(0);
///
///     thread::scope(|s| {
///         // Create the notify thread.
///         s.spawn(|| {
///             // Wake up some other threads after one second.
///             println!("[notify] Sleep for 1s");
///             thread::sleep(Duration::from_secs(1));
///             println!("[notify] Wake up main thread");
///             notify.store(NOTIFY_WAKE, Ordering::Relaxed);
///             wake_one(&notify);
///         });
///
///         // Main thread will wait until the notify thread wakes it up.
///         println!("[main] Waiting for notification..");
///         while notify.load(Ordering::Relaxed) == NOTIFY_WAIT {
///             wait(&notify, NOTIFY_WAIT);
///         }
///         println!("[main] Got wake up");
///     });
/// }
/// ```
pub unsafe fn futex(
    uaddr: &core::sync::atomic::AtomicU32,
    op: i32,
    val: u32,
    utime: Option<&timespec_t>,
    uaddr2: Option<&core::sync::atomic::AtomicU32>,
    val3: u32,
) -> Result<i32, Errno> {
    use core::sync::atomic::AtomicU32;
    let uaddr_ptr = uaddr as *const AtomicU32 as usize;
    let op = op as usize;
    let val = val as usize;
    let utime_ptr = utime.map_or(core::ptr::null::<timespec_t>() as usize, |time_ref| {
        time_ref as *const timespec_t as usize
    });
    let uaddr2_ptr = uaddr2.map_or(core::ptr::null::<AtomicU32>() as usize, |uaddr2_ref| {
        uaddr2_ref as *const AtomicU32 as usize
    });
    let val3 = val3 as usize;
    syscall6(SYS_FUTEX, uaddr_ptr, op, val, utime_ptr, uaddr2_ptr, val3).map(|ret| ret as i32)
}
