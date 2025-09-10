/// Move all pages in a process to another set of nodes
pub unsafe fn migrate_pages(
    pid: pid_t,
    max_node: usize,
    old_nodes: &[usize],
    new_nodes: &[usize],
) -> Result<isize, Errno> {
    let pid = pid as usize;
    let old_nodes = old_nodes.as_ptr() as usize;
    let new_nodes = new_nodes.as_ptr() as usize;
    syscall4(SYS_MIGRATE_PAGES, pid, max_node, old_nodes, new_nodes).map(|ret| ret as isize)
}
