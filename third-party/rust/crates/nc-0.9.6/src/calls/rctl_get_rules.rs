/// Returns in outbufp a comma-separated list of rules from the resource limits
/// database that match the filter passed in inbuf.
pub unsafe fn rctl_get_rules(inbuf: &[u8], outbuf: &mut [u8]) -> Result<(), Errno> {
    let inbuf_ptr = inbuf.as_ptr() as usize;
    let inbuf_len = inbuf.len();
    let outbuf_ptr = outbuf.as_mut_ptr() as usize;
    let outbuf_len = outbuf.len();
    syscall4(
        SYS_RCTL_GET_RULES,
        inbuf_ptr,
        inbuf_len,
        outbuf_ptr,
        outbuf_len,
    )
    .map(drop)
}
