/// Setup restartable sequences for caller thread.
pub unsafe fn rseq(rseq: &mut [rseq_t], flags: i32, sig: u32) -> Result<i32, Errno> {
    let rseq_ptr = rseq.as_mut_ptr() as usize;
    let rseq_len = rseq.len();
    let flags = flags as usize;
    let sig = sig as usize;
    syscall4(SYS_RSEQ, rseq_ptr, rseq_len, flags, sig).map(|ret| ret as i32)
}
