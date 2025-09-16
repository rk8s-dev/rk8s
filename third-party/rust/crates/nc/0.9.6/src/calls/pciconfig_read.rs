/// PCI device information handling.
pub unsafe fn pciconfig_read(
    bus: usize,
    dfn: usize,
    off: usize,
    len: usize,
    buf: usize,
) -> Result<(), Errno> {
    syscall5(SYS_PCICONFIG_READ, bus, dfn, off, len, buf).map(drop)
}
