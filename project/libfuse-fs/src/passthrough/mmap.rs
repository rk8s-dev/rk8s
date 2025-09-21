use std::{
    cmp,
    fs::File,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    sync::Arc,
};

use libc::{_SC_PAGESIZE, O_RDWR, O_WRONLY, fcntl, sysconf};
use memmap2::MmapOptions;
use tokio::sync::RwLock;

use crate::passthrough::Inode;

const HUGE_PAGE_LIMIT: u64 = 4 * 1024 * 1024;
pub const MAX_WINDOW_SIZE: usize = 40 * 1024 * 1024;

#[derive(PartialEq, Eq, Hash)]
pub struct MmapChunkKey {
    pub inode: Inode,
    pub aligned_offset: u64,
    pub page_size: u64,
}

impl MmapChunkKey {
    pub fn new(inode: Inode, offset: u64, file_size: u64) -> Self {
        let page_size = get_effective_page_size(file_size);
        let aligned_offset = align_down(offset, page_size);
        Self {
            inode,
            aligned_offset,
            page_size,
        }
    }

    #[allow(dead_code)]
    pub fn contains(&self, mmap_len: usize, offset: u64) -> bool {
        let start = self.aligned_offset;
        let end = start + mmap_len as u64;
        offset >= start && offset < end
    }
}

pub enum MmapCachedValue {
    Mmap(memmap2::Mmap),
    MmapMut(memmap2::MmapMut),
}

/// get system page size
pub fn get_page_size() -> Option<usize> {
    let size = unsafe { sysconf(_SC_PAGESIZE) };
    if size > 0 { Some(size as usize) } else { None }
}

/// align to system page size
pub fn align_to_page_size(size: u64, page_size: u64) -> u64 {
    size.div_ceil(page_size) * page_size
}

/// align down to system page size
pub fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

/// get effective page size (considering huge pages)
pub fn get_effective_page_size(file_size: u64) -> u64 {
    if file_size >= HUGE_PAGE_LIMIT {
        2 * 1024 * 1024 // 2MB huge page
    } else {
        get_page_size().unwrap_or(4096) as u64 // default 4KB page
    }
}

/// create mmap mapping
///
/// # arguments
/// - `offset`: the offset to map the file from
/// - `file`: the file to map
///
/// # returns
/// - `Ok(Arc<RwLock<Mmap>>)`: the created mmap mapping
/// - `Err(std::io::Error)`: the error information if creation fails
pub async fn create_mmap(
    offset: u64,
    file: &File,
) -> Result<Arc<RwLock<MmapCachedValue>>, std::io::Error> {
    let page_size =
        get_page_size().ok_or_else(|| std::io::Error::other("Failed to get page size"))? as u64;

    let file_size = file.metadata()?.size();

    // Check if offset exceeds file size
    if offset >= file_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Offset {offset} exceeds file size {file_size}"),
        ));
    }

    // Decide whether to use huge pages
    let (effective_page_size, huge_page) = if file_size >= HUGE_PAGE_LIMIT {
        (2 * 1024 * 1024, Some(21))
    } else {
        (page_size, None)
    };

    // Calculate aligned offset (downward alignment to page boundary)
    let aligned_offset = align_down(offset, effective_page_size);

    // Calculate maximum length to map (not exceeding file end)
    let max_available = file_size - aligned_offset;

    // Calculate chunk size, ensuring it does not exceed file end and maximum window size
    let raw_chunk_size = cmp::min(max_available, MAX_WINDOW_SIZE as u64);
    let chunk_size = align_to_page_size(raw_chunk_size, effective_page_size);

    // Ensure chunk size is not zero
    if chunk_size == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Cannot create mmap with zero length",
        ));
    }

    let flags = unsafe { fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags == -1 {
        return Err(std::io::Error::other(
            "Failed to get file access mode with fcntl",
        ));
    }
    let accmode = flags & libc::O_ACCMODE;
    let writeable = (accmode == O_WRONLY) || (accmode == O_RDWR);

    // Create mmap mapping
    let mmap_result: Result<MmapCachedValue, std::io::Error> = if writeable {
        let v = unsafe {
            MmapOptions::new()
                .len(chunk_size as usize)
                .offset(aligned_offset)
                .populate()
                .huge(huge_page)
                .map_mut(file)
        };
        match v {
            Ok(v) => Ok(MmapCachedValue::MmapMut(v)),
            Err(e) => Err(e),
        }
    } else {
        let v = unsafe {
            MmapOptions::new()
                .len(chunk_size as usize)
                .offset(aligned_offset)
                .populate()
                .huge(huge_page)
                .map(file)
        };
        match v {
            Ok(v) => Ok(MmapCachedValue::Mmap(v)),
            Err(e) => Err(e),
        }
    };

    match mmap_result {
        Ok(mmap) => Ok(Arc::new(RwLock::new(mmap))),
        Err(e) => {
            // Convert mmap error to more specific error information
            let error_kind = match e.raw_os_error() {
                Some(libc::ENOMEM) => std::io::ErrorKind::OutOfMemory,
                Some(libc::EINVAL) => std::io::ErrorKind::InvalidInput,
                Some(libc::EACCES) => std::io::ErrorKind::PermissionDenied,
                Some(libc::EBADF) => std::io::ErrorKind::NotFound,
                _ => std::io::ErrorKind::Other,
            };
            Err(std::io::Error::new(
                error_kind,
                format!("Failed to create mmap: {e}"),
            ))
        }
    }
}
