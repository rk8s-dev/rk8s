/// Terminate thread.
pub unsafe fn thr_exit(state: Option<&mut isize>) -> ! {
    let state_ptr = state.map_or(0, |state| state as *mut isize as usize);
    let _ret = syscall1(SYS_THR_EXIT, state_ptr).map(drop);
    unreachable!();
}
