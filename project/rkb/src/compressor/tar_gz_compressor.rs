use std::{
    ffi::OsStr,
    fs::{self, File, Metadata},
    io::{self, BufReader, BufWriter, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt},
    },
    path::Path,
};

use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use rand::{Rng, distr::Alphanumeric};
use sha2::{Digest, Sha256};
use tar::{Builder, Header};
use walkdir::WalkDir;

use crate::compressor::LayerCompressor;

use super::{LayerCompressionConfig, LayerCompressionResult};

/// A wrapper around a `Write`able object that calculates the SHA256 hash and total size
/// of the data being written, all in a single pass.
///
/// When creating a compressed tar archive (`.tar.gz`), we often need two pieces of
/// information for both the uncompressed tar data and the final compressed gzip data:
/// 1. The total size in bytes.
/// 2. The SHA256 checksum.
///
/// A naive approach would be to first write the data to a file, then read it back
/// entirely to calculate its size and hash. This is highly inefficient as it requires
/// double the I/O operations.
///
/// `HashingWriter` solves this by implementing the `Write` trait itself. It acts as a
/// "pass-through" adapter. When data is written to `HashingWriter`, it performs three
/// actions simultaneously:
/// - It passes the data to the inner `writer` (e.g., a file or another encoder).
/// - It updates its internal `Sha256` hasher with the same data.
/// - It increments its internal `size` counter.
///
/// This allows us to calculate the hash and size "on the fly" as the data streams
/// through, avoiding extra I/O passes and unnecessary memory buffering. In this file,
/// it is used in a chain (`tar -> gzip -> file`) to get metrics for both the raw tar
/// data and the final gzipped data efficiently.
struct HashingWriter<W: Write> {
    writer: W,
    hasher: Sha256,
    size: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: Sha256::new(),
            size: 0,
        }
    }

    fn finalize(self) -> (W, String, u64) {
        let hash = format!("{:x}", self.hasher.finalize());
        (self.writer, hash, self.size)
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[derive(Debug, Default)]
pub struct TarGzCompressor;

impl TarGzCompressor {
    /// Skip virtual file system
    fn should_skip_path(path: &Path) -> bool {
        let vfs_roots = [
            Path::new("/proc"),
            Path::new("/sys"),
            Path::new("/dev"),
            Path::new("/run"),
        ];

        for vfs_root in &vfs_roots {
            if path.starts_with(vfs_root) {
                return true;
            }
        }
        false
    }

    /// Add regular file
    fn append_file<W: Write>(
        &self,
        builder: &mut Builder<W>,
        path: &Path,
        name: &Path,
        metadata: &Metadata,
    ) -> Result<()> {
        let file =
            File::open(path).with_context(|| format!("Cannot open file: {}", path.display()))?;
        let mut file = BufReader::with_capacity(1024 * 1024, file);
        let mut header = Header::new_gnu();
        header.set_metadata(metadata);
        header.set_path(name)?;
        header.set_size(metadata.len());
        header.set_cksum();

        builder
            .append(&header, &mut file)
            .with_context(|| format!("Failed to append file {} to tar archive", path.display()))
    }

    /// Add directory
    fn append_dir<W: Write>(
        &self,
        builder: &mut Builder<W>,
        path: &Path,
        name: &Path,
        metadata: &Metadata,
    ) -> Result<()> {
        let mut header = Header::new_gnu();
        header.set_metadata(metadata);
        let dir_name_bytes = name.as_os_str().as_bytes();
        let dir_name_with_slash = if dir_name_bytes.last() == Some(&b'/') {
            name.as_os_str().to_owned()
        } else {
            let mut new_name = dir_name_bytes.to_vec();
            new_name.push(b'/');
            OsStr::from_bytes(&new_name).to_owned()
        };
        header.set_path(Path::new(&dir_name_with_slash))?;
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_cksum();

        builder.append(&header, &mut io::empty()).with_context(|| {
            format!(
                "Failed to append directory {} to tar archive",
                path.display()
            )
        })
    }

    /// Add symbolic link
    fn append_symlink<W: Write>(
        &self,
        builder: &mut Builder<W>,
        path: &Path,
        name: &Path,
    ) -> Result<()> {
        let target = fs::read_link(path)?;
        let mut header = Header::new_gnu();
        let metadata = fs::symlink_metadata(path)?;
        header.set_metadata(&metadata);
        header.set_path(name)?;
        header.set_link_name(&target)?;
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();

        builder
            .append(&header, &mut io::empty())
            .with_context(|| format!("Failed to append symlink {} to tar archive", path.display()))
    }

    /// Add special file
    fn append_special_file<W: Write>(
        &self,
        builder: &mut Builder<W>,
        path: &Path,
        name: &Path,
        metadata: &Metadata,
    ) -> Result<()> {
        let mut header = Header::new_gnu();
        header.set_metadata(metadata);
        header.set_path(name)?;
        header.set_size(0);
        let file_type = metadata.file_type();
        if file_type.is_block_device() {
            header.set_entry_type(tar::EntryType::Block);
        } else if file_type.is_char_device() {
            header.set_entry_type(tar::EntryType::Char);
        } else if file_type.is_file() {
            header.set_entry_type(tar::EntryType::Fifo);
        } else if file_type.is_socket() {
            header.set_entry_type(tar::EntryType::Regular);
        }
        if file_type.is_block_device() || file_type.is_char_device() {
            let dev_major = (metadata.rdev() >> 8) & 0xFFF;
            let dev_minor = metadata.rdev() & 0xFF;
            header.set_device_major(dev_major as _)?;
            header.set_device_minor(dev_minor as _)?;
        }
        header.set_cksum();

        builder
            .append(&header, &mut io::empty())
            .with_context(|| format!("Failed to append symlink {} to tar archive", path.display()))
    }

    fn create_tar_and_compress(
        &self,
        source_path: &Path,
        gz_path: &Path,
    ) -> Result<(String, u64, String, u64)> {
        if !source_path.exists() {
            bail!("Source path doesn't exist: {}", source_path.display());
        }
        if !source_path.is_dir() {
            bail!("Source path is not a directory: {}", source_path.display());
        }

        let gz_file = File::create(gz_path)?;
        let gz_writer = BufWriter::with_capacity(1024 * 1024, gz_file);
        let gz_hashing_writer = HashingWriter::new(gz_writer);

        let gz_encoder = GzEncoder::new(gz_hashing_writer, Compression::fast());
        let tar_hashing_writer = HashingWriter::new(gz_encoder);

        let mut tar_builder = Builder::new(tar_hashing_writer);

        for entry_result in WalkDir::new(source_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !Self::should_skip_path(e.path()))
        {
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::error!("Error in walkdir: {err}");
                    continue;
                }
            };

            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => {
                    tracing::error!("Failed to get metadata from {}", path.display());
                    continue;
                }
            };

            // relative path used in tar file
            let relative_path = match path.strip_prefix(source_path) {
                Ok(rel_path) => rel_path,
                Err(_) => {
                    continue;
                }
            };

            // skip source directory itself
            if relative_path.as_os_str().is_empty() {
                continue;
            }

            let _result = if metadata.is_file() {
                self.append_file(&mut tar_builder, path, relative_path, &metadata)
            } else if metadata.is_dir() {
                self.append_dir(&mut tar_builder, path, relative_path, &metadata)
            } else if metadata.file_type().is_symlink() {
                self.append_symlink(&mut tar_builder, path, relative_path)
            } else if metadata.file_type().is_block_device()
                || metadata.file_type().is_char_device()
                || metadata.file_type().is_fifo()
                || metadata.file_type().is_socket()
            {
                // use unix header
                self.append_special_file(&mut tar_builder, path, relative_path, &metadata)
            } else {
                tracing::warn!("Skip unknown file type: {}", path.display());
                continue;
            };
        }

        let tar_hashing_writer = tar_builder.into_inner()?;
        let (gz_encoder, tar_hash, tar_size) = tar_hashing_writer.finalize();
        let gz_hashing_writer = gz_encoder.finish()?;
        let (_buf_writer, gz_hash, gz_size) = gz_hashing_writer.finalize();

        Ok((tar_hash, tar_size, gz_hash, gz_size))
    }
}

impl LayerCompressor for TarGzCompressor {
    fn compress_layer(
        &self,
        compression_config: &LayerCompressionConfig,
    ) -> Result<LayerCompressionResult> {
        let source_dir = &compression_config.layer_dir;
        tracing::info!("Compressing layer {}", source_dir.display());

        // use a random string as tar file name
        let rng = rand::rng();
        let random_string: String = rng
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();

        let gz_path = compression_config
            .output_dir
            .join(format!("{}.tar.gz", &random_string));

        let (tar_sha256sum, tar_size, gz_sha256sum, gz_size) =
            self.create_tar_and_compress(source_dir, &gz_path)?;

        let formatted_gz_path = compression_config.output_dir.join(&gz_sha256sum);
        fs::rename(&gz_path, &formatted_gz_path).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                gz_path.display(),
                formatted_gz_path.display()
            )
        })?;

        Ok(LayerCompressionResult::new(
            tar_sha256sum,
            tar_size,
            gz_sha256sum,
            gz_size,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Instant};

    use rayon::prelude::*;
    use tempfile::tempdir;

    use crate::compressor::{LayerCompressionConfig, LayerCompressor};

    #[test]
    fn test_compression() {
        let tmp_dir = tempdir().unwrap();
        let layer_dir = tmp_dir.path().to_path_buf();

        let layer_file = layer_dir.join("file.txt");
        fs::write(&layer_file, "Hello, world!").unwrap();

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compression_config = LayerCompressionConfig::new(layer_dir, output_dir);

        let compressor1 = super::TarGzCompressor;
        let compression_result1 = compressor1.compress_layer(&compression_config).unwrap();

        let tmp_dir = tempdir().unwrap();
        let layer_dir = tmp_dir.path().to_path_buf();

        let layer_file = layer_dir.join("file.txt");
        fs::write(&layer_file, "Hello, world!").unwrap();

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compression_config = LayerCompressionConfig::new(layer_dir, output_dir);

        let compressor2 = super::TarGzCompressor;
        let compression_result2 = compressor2.compress_layer(&compression_config).unwrap();

        assert_eq!(
            compression_result1.tar_sha256sum,
            compression_result2.tar_sha256sum
        );
        assert_eq!(compression_result1.tar_size, compression_result2.tar_size);
        assert_eq!(
            compression_result1.gz_sha256sum,
            compression_result2.gz_sha256sum
        );
        assert_eq!(compression_result1.gz_size, compression_result2.gz_size);
    }

    #[test]
    fn test_compress_fs() {
        let layer_path = std::path::Path::new("/home/yu/layers/lower");

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compression_config = LayerCompressionConfig::new(layer_path.to_path_buf(), output_dir);
        let compressor1 = super::TarGzCompressor;
        let compression_result1 = compressor1.compress_layer(&compression_config).unwrap();

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compressor2 = super::TarGzCompressor;
        let compression_config = LayerCompressionConfig::new(layer_path.to_path_buf(), output_dir);
        let compression_result2 = compressor2.compress_layer(&compression_config).unwrap();

        assert_eq!(
            compression_result1.tar_sha256sum,
            compression_result2.tar_sha256sum
        );
        assert_eq!(compression_result1.tar_size, compression_result2.tar_size);
        assert_eq!(
            compression_result1.gz_sha256sum,
            compression_result2.gz_sha256sum
        );
        assert_eq!(compression_result1.gz_size, compression_result2.gz_size);
    }

    #[test]
    #[ignore]
    fn test_compression_performance() {
        let layer_path = std::path::Path::new("/home/yu/layers/lower");
        let compressor = super::TarGzCompressor;

        // --- 1. Serial Execution ---
        println!("Starting serial compression...");
        let start_serial = Instant::now();

        // First compression
        let tmp_dir1 = tempdir().unwrap();
        let config1 =
            LayerCompressionConfig::new(layer_path.to_path_buf(), tmp_dir1.path().to_path_buf());
        compressor.compress_layer(&config1).unwrap();

        // Second compression
        let tmp_dir2 = tempdir().unwrap();
        let config2 =
            LayerCompressionConfig::new(layer_path.to_path_buf(), tmp_dir2.path().to_path_buf());
        compressor.compress_layer(&config2).unwrap();

        let serial_duration = start_serial.elapsed();
        println!("Serial execution time (2 tasks): {:?}", serial_duration);

        // --- 2. Parallel Execution with Rayon ---
        println!("\nStarting parallel compression...");
        let start_parallel = Instant::now();

        let configs = [config1, config2]; // Reuse configs with different output dirs

        configs.par_iter().for_each(|config| {
            let compressor = super::TarGzCompressor;
            // Each parallel task needs its own output directory.
            let output_dir = tempdir().unwrap();
            let parallel_config = LayerCompressionConfig::new(
                config.layer_dir.clone(),
                output_dir.path().to_path_buf(),
            );
            compressor.compress_layer(&parallel_config).unwrap();
        });

        let parallel_duration = start_parallel.elapsed();
        println!("Parallel execution time (2 tasks): {:?}", parallel_duration);
    }
}
