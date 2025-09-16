/// Adds the rule pointed to by inbuf to the resource limits database.
pub unsafe fn rctl_add_rule(inbuf: &[u8], outbuf: &mut [u8]) -> Result<(), Errno> {
    let inbuf_ptr = inbuf.as_ptr() as usize;
    let inbuf_len = inbuf.len();
    let outbuf_ptr = outbuf.as_mut_ptr() as usize;
    let outbuf_len = outbuf.len();
    syscall4(
        SYS_RCTL_ADD_RULE,
        inbuf_ptr,
        inbuf_len,
        outbuf_ptr,
        outbuf_len,
    )
    .map(drop)
}
