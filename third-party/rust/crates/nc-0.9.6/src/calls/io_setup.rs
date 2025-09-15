/// Create an asynchronous I/O context.
///
/// Create an `aio_context` capable of receiving at least `nr_events`.
/// `ctx_id` must not point to an `aio_context` that already exists, and
/// must be initialized to 0 prior to the call.
///
/// On successful creation of the `aio_context`, `ctx_id` is filled in with
/// the resulting handle.
///
/// # Errors
///
/// - May fail with `-EINVAL` if `*ctx_id` is not initialized,
///   if the specified `nr_events` exceeds internal limits.
/// - May fail with `-EAGAIN` if the specified `nr_events` exceeds the user's limit
///   of available events.
/// - May fail with `-ENOMEM` if insufficient kernel resources are available.
/// - May fail with `-EFAULT` if an invalid pointer is passed for `ctx_id`.
/// - Will fail with `-ENOSYS` if not implemented.
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
/// let out_filename = "/tmp/nc-io-setup";
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
/// let ret = unsafe { nc::io_getevents(ctx, 1, &mut events, Some(&timeout)) };
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
pub unsafe fn io_setup(nr_events: u32, ctx_id: &mut aio_context_t) -> Result<(), Errno> {
    let nr_events = nr_events as usize;
    let ctx_id_ptr = ctx_id as *mut aio_context_t as usize;
    syscall2(SYS_IO_SETUP, nr_events, ctx_id_ptr).map(drop)
}
