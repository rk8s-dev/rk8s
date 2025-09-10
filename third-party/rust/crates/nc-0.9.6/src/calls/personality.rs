/// Set the process execution domain.
///
/// Returns old execution domain.
pub unsafe fn personality(persona: u32) -> Result<u32, Errno> {
    let persona = persona as usize;
    syscall1(SYS_PERSONALITY, persona).map(|ret| ret as u32)
}
