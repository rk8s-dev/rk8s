use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd, RawFd},
};

use crate::passthrough::statx;

/// Sliding mmap window structure for large files
pub struct SlidingMmapWindow {
    fd: RawFd,
    window_start: u64,
    window_end: u64,
    window_size: u64,
    mmap: memmap2::MmapMut,
}
impl SlidingMmapWindow {
    pub async fn new(file: &impl AsRawFd, window_size: u64) -> io::Result<SlidingMmapWindow> {
        let fd = file.as_raw_fd();
        let mmap = Self::create_mmap(file).await?;
        Ok(Self {
            fd,
            window_start: 0,
            window_end: window_size,
            window_size,
            mmap,
        })
    }

    async fn create_mmap(file: &impl AsRawFd) -> rfuse3::Result<memmap2::MmapMut> {
        const USE_HUGE_PAGE: u64 = 10 * 1024 * 1024; // 10MB
        const MAX_WINDOW_SIZE: u64 = 100 * 1024 * 1024; // 100MB

        let st = statx::statx(file, None)?;
        let file_size = st.st.st_size as u64;

        let map_size = file_size.min(MAX_WINDOW_SIZE);

        let page_bits: Option<u8> = if file_size >= USE_HUGE_PAGE {
            Some(21) // Use huge pages for large files
        } else {
            None // Use regular pages for smaller files
        };

        // Create mmap with appropriate options
        let mmap = unsafe {
            memmap2::MmapOptions::new()
                .len(map_size as usize)
                .huge(page_bits)
                .populate() // Pre-populate pages to avoid page faults
                .map_mut(file)
        }
        .map_err(|e| {
            io::Error::other(format!(
                "Failed to create mmap for file size {file_size}: {e}",
            ))
        })?;

        Ok(mmap)
    }

    /// Slide the window to cover the requested range
    pub fn slide_window(&mut self, offset: u64, size: usize) -> io::Result<()> {
        let requested_end = offset + size as u64;

        // If current window doesn't cover the requested range, create new window
        if offset < self.window_start || requested_end > self.window_end {
            // Use statx to get file size without creating File object
            let file = unsafe { File::from_raw_fd(self.fd) };
            let file_size = file.metadata()?.len();
            // Don't let the File object drop - we don't own the fd
            std::mem::forget(file);

            let new_window_start = (offset / self.window_size) * self.window_size;
            let new_window_end = std::cmp::min(new_window_start + self.window_size, file_size);

            unsafe {
                let temp_file = File::from_raw_fd(self.fd);
                let mmap = memmap2::MmapOptions::new()
                    .offset(new_window_start)
                    .len((new_window_end - new_window_start) as usize)
                    .map_mut(&temp_file)?;
                // Don't let temp_file drop - we don't own the fd
                std::mem::forget(temp_file);

                self.mmap = mmap;
                self.window_start = new_window_start;
                self.window_end = new_window_end;
            }
        }
        Ok(())
    }

    pub fn check_slide(&self, offset: u64, size: usize) -> bool {
        !(offset >= self.window_start && offset + size as u64 <= self.window_end)
    }

    /// Read data from the current window
    pub fn read(&self, offset: u64, size: usize, buf: &mut [u8]) -> io::Result<usize> {
        if offset >= self.window_start && offset + size as u64 <= self.window_end {
            let start = (offset - self.window_start) as usize;
            let end = start + size;
            if end <= self.mmap.len() {
                buf.copy_from_slice(&self.mmap[start..end]);
                return Ok(size);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Read outside window",
        ))
    }

    /// Write data to the current window
    pub fn write(&mut self, offset: u64, data: &[u8]) -> io::Result<usize> {
        if offset >= self.window_start && offset + data.len() as u64 <= self.window_end {
            let start = (offset - self.window_start) as usize;
            let end = start + data.len();
            if end <= self.mmap.len() {
                self.mmap[start..end].copy_from_slice(data);
                return Ok(data.len());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Write outside window",
        ))
    }

    pub fn window_size(&self) -> u64 {
        self.window_size
    }
}
