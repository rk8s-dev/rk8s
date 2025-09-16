/// Append the message to a System V message queue.
///
/// # Examples
///
/// ```
/// const MAX_MTEXT: usize = 1024;
///
/// const MTYPE_NULL: isize = 0;
/// const MTYPE_CLIENT: isize = 1;
/// const _MTYPE_SERVER: isize = 2;
///
/// #[derive(Debug, Clone, Copy)]
/// #[repr(C)]
/// struct Message {
///     pub mtype: isize,
///     pub mtext: [u8; MAX_MTEXT],
/// }
///
/// impl Default for Message {
///     fn default() -> Self {
///         Message {
///             mtype: MTYPE_NULL,
///             mtext: [0; MAX_MTEXT],
///         }
///     }
/// }
///
/// fn main() {
///     let key = nc::IPC_PRIVATE;
///     let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
///     let ret = unsafe { nc::msgget(key, flags) };
///     assert!(ret.is_ok());
///     let msq_id = ret.unwrap();
///
///     // Write to message queue.
///     let msg = "Hello, Rust";
///     let mut client_msg = Message {
///         mtype: MTYPE_CLIENT,
///         mtext: [0; MAX_MTEXT],
///     };
///     let msg_len = msg.len();
///     unsafe {
///         let src_ptr = msg.as_ptr();
///         let dst_ptr = client_msg.mtext.as_mut_ptr();
///         core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, msg_len);
///     }
///
///     let ret = unsafe { nc::msgsnd(msq_id, &client_msg as *const Message as *const _, msg_len, 0) };
///     assert!(ret.is_ok());
///
///     // Read from message queue.
///     let mut recv_msg = Message::default();
///     let ret = unsafe {
///         nc::msgrcv(
///             msq_id,
///             &mut recv_msg as *mut Message as *mut _,
///             MAX_MTEXT,
///             MTYPE_CLIENT,
///             0,
///         )
///     };
///     assert!(ret.is_ok());
///     let recv_msg_len = ret.unwrap() as usize;
///     assert_eq!(recv_msg_len, msg_len);
///     let recv_text = core::str::from_utf8(&recv_msg.mtext[..recv_msg_len]);
///     assert!(recv_text.is_ok());
///     let recv_text = recv_text.unwrap();
///     assert_eq!(recv_text, msg);
///
///     let mut buf = nc::msqid_ds_t::default();
///     let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn msgsnd(
    msq_id: i32,
    msgq: *const core::ffi::c_void,
    msg_size: size_t,
    msg_flag: i32,
) -> Result<(), Errno> {
    let msq_id = msq_id as usize;
    let msgq = msgq as usize;
    let msg_flag = msg_flag as usize;
    syscall4(SYS_MSGSND, msq_id, msgq, msg_size, msg_flag).map(drop)
}
