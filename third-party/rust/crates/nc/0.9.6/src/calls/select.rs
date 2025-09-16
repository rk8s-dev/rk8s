/// Sychronous I/O multiplexing.
pub unsafe fn select(
    nfds: i32,
    read_fds: Option<&mut fd_set_t>,
    write_fds: Option<&mut fd_set_t>,
    except_fds: Option<&mut fd_set_t>,
    timeout: Option<&timeval_t>,
) -> Result<i32, Errno> {
    use core::ptr::{null, null_mut};

    let nfds = nfds as usize;
    let read_fds_ptr = read_fds.map_or(null_mut::<fd_set_t>() as usize, |read_fds| {
        read_fds as *mut fd_set_t as usize
    });
    let write_fds_ptr = write_fds.map_or(null_mut::<fd_set_t>() as usize, |write_fds| {
        write_fds as *mut fd_set_t as usize
    });
    let except_fds_ptr = except_fds.map_or(null_mut::<fd_set_t>() as usize, |except_fds| {
        except_fds as *mut fd_set_t as usize
    });
    let timeout_ptr = timeout.map_or(null::<timeval_t> as usize, |timeout| {
        timeout as *const timeval_t as usize
    });
    syscall5(
        SYS_SELECT,
        nfds,
        read_fds_ptr,
        write_fds_ptr,
        except_fds_ptr,
        timeout_ptr,
    )
    .map(|ret| ret as i32)
}
