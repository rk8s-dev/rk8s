/// Wait for process to change state.
pub unsafe fn wait6(
    idtype: idtype_t,
    id: id_t,
    status: &mut i32,
    options: i32,
    wrusage: &mut wrusage_t,
    info: &mut siginfo_t,
) -> Result<pid_t, Errno> {
    let idtype = idtype as usize;
    let id = id as usize;
    let status_ptr = status as *mut i32 as usize;
    let options = options as usize;
    let wrusage_ptr = wrusage as *mut wrusage_t as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall6(
        SYS_WAIT6,
        idtype,
        id,
        status_ptr,
        options,
        wrusage_ptr,
        info_ptr,
    )
    .map(|ret| ret as pid_t)
}
