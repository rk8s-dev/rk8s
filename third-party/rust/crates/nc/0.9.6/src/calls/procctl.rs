/// Control process
pub unsafe fn procctl(idtype: idtype_t, id: id_t, cmd: i32, data: usize) -> Result<(), Errno> {
    let idtype = idtype as usize;
    let id = id as usize;
    let cmd = cmd as usize;
    syscall4(SYS_PROCCTL, idtype, id, cmd, data).map(drop)
}
