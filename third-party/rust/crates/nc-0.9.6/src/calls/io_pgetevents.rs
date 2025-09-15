/// Read asynchronous I/O events from the completion queue
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-getevents";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_pgetevents(ctx, 1, &mut events, Some(&timeout), None) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_pgetevents(
    ctx_id: aio_context_t,
    min_nr: isize,
    events: &mut [io_event_t],
    timeout: Option<&timespec_t>,
    sig: Option<&aio_sigset_t>,
) -> Result<i32, Errno> {
    use core::ptr::null;

    let min_nr = min_nr as usize;
    let nr = events.len();
    let events_ptr = events.as_mut_ptr() as usize;
    let timeout_ptr = timeout.map_or(null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sig_ptr = sig.map_or(null::<aio_sigset_t>() as usize, |sig| {
        sig as *const aio_sigset_t as usize
    });
    syscall6(
        SYS_IO_PGETEVENTS,
        ctx_id,
        min_nr,
        nr,
        events_ptr,
        timeout_ptr,
        sig_ptr,
    )
    .map(|ret| ret as i32)
}
