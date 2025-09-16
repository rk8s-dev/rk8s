/// `cachestat()` returns the page cache statistics of a file in the
/// bytes range specified by `off` and `len`.
///
/// Number of cached pages, number of dirty pages, number of pages marked for writeback,
/// number of evicted pages, and number of recently evicted pages.
///
/// An evicted page is a page that is previously in the page cache
/// but has been evicted since. A page is recently evicted if its last
/// eviction was recent enough that its reentry to the cache would
/// indicate that it is actively being used by the system, and that
/// there is memory pressure on the system.
///
/// `off` and `len` must be non-negative integers. If `len` > 0,
/// the queried range is [`off`, `off` + `len`]. If `len` == 0,
/// we will query in the range from `off` to the end of the file.
///
/// The `flags` argument is unused for now, but is included for future
/// extensibility. User should pass 0 (i.e no flag specified).
///
/// Currently, hugetlbfs is not supported.
///
/// Because the status of a page can change after `cachestat()` checks it
/// but before it returns to the application, the returned values may
/// contain stale information.
///
/// return values:
/// - zero       - success
/// - EFAULT     - cstat or `cstat_range` points to an illegal address
/// - EINVAL     - invalid flags
/// - EBADF      - invalid file descriptor
/// - EOPNOTSUPP - file descriptor is of a hugetlbfs file
pub unsafe fn cachestat(
    fd: u32,
    cstat_range: &mut cachestat_range_t,
    cstat: &mut cachestat_t,
    flags: u32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cstat_range_ptr = cstat_range as *mut cachestat_range_t as usize;
    let cstat_ptr = cstat as *mut cachestat_t as usize;
    let flags = flags as usize;
    syscall4(SYS_CACHESTAT, fd, cstat_range_ptr, cstat_ptr, flags).map(drop)
}
