/// Control process profiling
pub unsafe fn profil(samples: &mut [u8], offset: vm_offset_t, scale: i32) -> Result<(), Errno> {
    let samples_ptr = samples.as_mut_ptr() as usize;
    let size = samples.len();
    let scale = scale as usize;
    syscall4(SYS_PROFIL, samples_ptr, size, offset, scale).map(drop)
}
