/// Sychronous I/O multiplexing.
///
/// Most architectures can't handle 7-argument syscalls. So we provide a
/// 6-argument version where the sixth argument is a pointer to a structure
/// which has a pointer to the `sigset_t` itself followed by a `size_t` containing
/// the sigset size.
pub unsafe fn pselect6(
    nfds: i32,
    read_fds: Option<&mut fd_set_t>,
    write_fds: Option<&mut fd_set_t>,
    except_fds: Option<&mut fd_set_t>,
    timeout: Option<&timespec_t>,
    sigmask: Option<&sigset_t>,
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
    let timeout_ptr = timeout.map_or(null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sigmask_ptr = sigmask.map_or(null::<sigset_t>() as usize, |sigmask| {
        sigmask as *const sigset_t as usize
    });
    syscall6(
        SYS_PSELECT6,
        nfds,
        read_fds_ptr,
        write_fds_ptr,
        except_fds_ptr,
        timeout_ptr,
        sigmask_ptr,
    )
    .map(|ret| ret as i32)
}
