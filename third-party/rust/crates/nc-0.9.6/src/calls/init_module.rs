/// Load a kernel module.
pub unsafe fn init_module<P: AsRef<Path>>(
    module_image: &[u8],
    param_values: P,
) -> Result<(), Errno> {
    let module_image_ptr = module_image.as_ptr() as usize;
    let len = module_image.len();
    let param_values = CString::new(param_values.as_ref());
    let param_values_ptr = param_values.as_ptr() as usize;
    syscall3(SYS_INIT_MODULE, module_image_ptr, len, param_values_ptr).map(drop)
}
