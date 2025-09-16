/// PCI device information handling.
// TODO(Shaohua): Check return type.
pub unsafe fn pciconfig_iobase(which: isize, bus: usize, dfn: usize) -> Result<usize, Errno> {
    let which = which as usize;
    syscall3(SYS_PCICONFIG_IOBASE, which, bus, dfn)
}
