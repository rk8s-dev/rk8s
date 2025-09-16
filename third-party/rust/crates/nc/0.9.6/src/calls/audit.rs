/// Commit BSM audit record to audit log
pub unsafe fn audit(record: &[u8]) -> Result<(), Errno> {
    let record_ptr = record.as_ptr() as usize;
    let length = record.len();
    syscall2(SYS_AUDIT, record_ptr, length).map(drop)
}
