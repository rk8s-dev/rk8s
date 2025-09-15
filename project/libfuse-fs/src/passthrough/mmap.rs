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
const MAX_WINDOW_SIZE: usize = 40 * 1024 * 1024;

#[derive(PartialEq, Eq, Hash)]
pub struct MmapChunkKey {
    inode: Inode,
    offset: u64,
}

impl MmapChunkKey {
    pub fn new(inode: Inode, offset: u64) -> Self {
        Self { inode, offset }
    }
}

pub enum MmapCachedValue {
    Mmap(memmap2::Mmap),
    MmapMut(memmap2::MmapMut),
}

/// 获取系统页面大小
pub fn get_page_size() -> Option<usize> {
    let size = unsafe { sysconf(_SC_PAGESIZE) };
    if size > 0 { Some(size as usize) } else { None }
}

/// 向上对齐到页面大小
pub fn align_to_page_size(size: u64, page_size: u64) -> u64 {
    size.div_ceil(page_size) * page_size
}

/// 向下对齐到页面大小
pub fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

/// 获取有效的页面大小（考虑大页）
pub fn get_effective_page_size(file_size: u64) -> u64 {
    if file_size >= HUGE_PAGE_LIMIT {
        2 * 1024 * 1024 // 2MB大页
    } else {
        get_page_size().unwrap_or(4096) as u64 // 默认4KB页面
    }
}

/// 创建mmap映射
///
/// # 参数
/// - `offset`: 要映射的文件偏移量
/// - `file`: 要映射的文件
///
/// # 返回
/// - `Ok(Arc<RwLock<Mmap>>)`: 成功创建的mmap映射
/// - `Err(std::io::Error)`: 创建失败的错误信息
pub async fn create_mmap(
    offset: u64,
    file: &File,
) -> Result<Arc<RwLock<MmapCachedValue>>, std::io::Error> {
    let page_size =
        get_page_size().ok_or_else(|| std::io::Error::other("Failed to get page size"))? as u64;

    let file_size = file.metadata()?.size();

    // 检查偏移量是否超出文件范围
    if offset >= file_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Offset {} exceeds file size {}", offset, file_size),
        ));
    }

    // 决定是否使用大页
    let (effective_page_size, huge_page) = if file_size >= HUGE_PAGE_LIMIT {
        (2 * 1024 * 1024, Some(21))
    } else {
        (page_size, None)
    };

    // 计算对齐后的偏移量（向下对齐到页面边界）
    let aligned_offset = align_down(offset, effective_page_size);

    // 计算可映射的最大长度（不超过文件末尾）
    let max_available = file_size - aligned_offset;

    // 计算chunk大小，确保不超过文件末尾和最大窗口大小
    let raw_chunk_size = cmp::min(max_available, MAX_WINDOW_SIZE as u64);
    let chunk_size = align_to_page_size(raw_chunk_size, effective_page_size);

    // 确保chunk大小不为0
    if chunk_size == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Cannot create mmap with zero length",
        ));
    }

    let flags = unsafe { fcntl(file.as_raw_fd(), libc::F_GETFL) };
    let accmode = flags & libc::O_ACCMODE;
    let writeable = (accmode == O_WRONLY) || (accmode == O_RDWR);

    // 创建mmap映射
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
            // 将mmap错误转换为更具体的错误信息
            let error_kind = match e.raw_os_error() {
                Some(libc::ENOMEM) => std::io::ErrorKind::OutOfMemory,
                Some(libc::EINVAL) => std::io::ErrorKind::InvalidInput,
                Some(libc::EACCES) => std::io::ErrorKind::PermissionDenied,
                Some(libc::EBADF) => std::io::ErrorKind::NotFound,
                _ => std::io::ErrorKind::Other,
            };
            Err(std::io::Error::new(
                error_kind,
                format!("Failed to create mmap: {}", e),
            ))
        }
    }
}
