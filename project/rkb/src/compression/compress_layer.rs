use std::{
    fs::{self, File, Metadata},
    io::{self, BufReader, BufWriter},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
};

use anyhow::{Context, Result, anyhow};
use flate2::{Compression, write::GzEncoder};
use rand::{Rng, distr::Alphanumeric};
use sha256::try_digest;
use tar::{Builder, Header};
use walkdir::WalkDir;

use super::{
    layer_compression_config::LayerCompressionConfig,
    layer_compression_result::LayerCompressionResult,
};

/// Skip virtual file system
fn should_skip_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // skip contents instead of directory
    (path_str.contains("/proc/") && !path_str.ends_with("/proc"))
        || (path_str.contains("/sys/") && !path_str.ends_with("/sys"))
        || (path_str.contains("/dev/")
            && !path_str.ends_with("/dev")
            && !path_str.contains("/dev/null")
            && !path_str.contains("/dev/zero")
            && !path_str.contains("/dev/full")
            && !path_str.contains("/dev/random")
            && !path_str.contains("/dev/urandom")
            && !path_str.contains("/dev/tty")
            && !path_str.contains("/dev/console"))
        || (path_str.contains("/run/") && !path_str.ends_with("/run"))
}

/// Create tar file from layer directory, we should pay attention to symlink and special files
fn create_tar(source_path: &Path, tar_path: &Path) -> Result<()> {
    if !source_path.exists() {
        return Err(anyhow!(
            "Tar source directory doesn't exist: {}",
            source_path.display()
        ));
    }
    if !source_path.is_dir() {
        return Err(anyhow!("Tar source is not a directory"));
    }

    let file = File::create(tar_path)?;
    let mut tar_builder = Builder::new(file);

    for entry_result in WalkDir::new(source_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_path(e.path()))
    {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Error in walkdir: {err}");
                continue;
            }
        };

        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => {
                eprintln!("Failed to get metadata from {}", path.display());
                continue;
            }
        };

        // relative path used in tar file
        let relative_path = match path.strip_prefix(source_path) {
            Ok(rel_path) => rel_path.to_string_lossy(),
            Err(_) => {
                continue;
            }
        };

        // skip source directory itself
        if relative_path.is_empty() {
            continue;
        }

        let _result = if metadata.is_file() {
            append_file(&mut tar_builder, path, &relative_path, &metadata)
        } else if metadata.is_dir() {
            append_dir(&mut tar_builder, path, &relative_path, &metadata)
        } else if metadata.file_type().is_symlink() {
            append_symlink(&mut tar_builder, path, &relative_path)
        } else if metadata.file_type().is_block_device()
            || metadata.file_type().is_char_device()
            || metadata.file_type().is_fifo()
            || metadata.file_type().is_socket()
        {
            // use unix header
            append_special_file(&mut tar_builder, path, &relative_path, &metadata)
        } else {
            eprintln!("Skip unknown file type: {}", path.display());
            continue;
        };
    }
    tar_builder.finish()?;

    Ok(())
}

/// Add regular file
fn append_file(
    builder: &mut Builder<File>,
    path: &Path,
    name: &str,
    metadata: &Metadata,
) -> Result<()> {
    let file = File::open(path).context(format!("Cannot open file: {}", path.display()))?;
    let mut file = BufReader::new(file);
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
fn append_dir(
    builder: &mut Builder<File>,
    path: &Path,
    name: &str,
    metadata: &Metadata,
) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_metadata(metadata);
    let dir_name = if name.ends_with('/') {
        name.to_string()
    } else {
        format!("{name}/")
    };
    header.set_path(&dir_name)?;
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
fn append_symlink(builder: &mut Builder<File>, path: &Path, name: &str) -> Result<()> {
    let target = fs::read_link(path)?;
    let target_str = target.to_string_lossy();
    let mut header = Header::new_gnu();
    let metadata = fs::symlink_metadata(path)?;
    header.set_metadata(&metadata);
    header.set_path(name)?;
    header.set_link_name(target_str.as_ref())?;
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_cksum();

    builder
        .append(&header, &mut io::empty())
        .with_context(|| format!("Failed to append symlink {} to tar archive", path.display()))
}

/// Add special file
fn append_special_file(
    builder: &mut Builder<File>,
    path: &Path,
    name: &str,
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

/// Compress file to gzip
fn compress_to_gz(tar_path: &Path, gz_path: &Path) -> Result<()> {
    if !tar_path.exists() {
        return Err(anyhow!(format!(
            "Tar file path doesn't exist: {}",
            tar_path.display()
        )));
    }

    println!(
        "Compressing {} to {}",
        tar_path.display(),
        gz_path.display()
    );

    let tar_file = File::open(tar_path)?;
    let mut tar_file = BufReader::new(tar_file);

    let gz_file = File::create(gz_path)?;
    let gz_file = BufWriter::new(gz_file);

    let mut encoder = GzEncoder::new(gz_file, Compression::best());
    // might be useful
    let _bytes = io::copy(&mut tar_file, &mut encoder)?;
    encoder.finish()?;

    Ok(())
}

/// Compress layer to tar.gz
///
/// Returns the size and sha256sum of the result
pub fn compress(compression_config: &LayerCompressionConfig) -> Result<LayerCompressionResult> {
    let source_dir = &compression_config.layer_dir;
    println!("Compressing layer {}", source_dir.display());

    // use a random string as tar file name
    let rng = rand::rng();
    let random_string: String = rng
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let tar_path = compression_config
        .output_dir
        .join(format!("{}.tar", &random_string));
    let gz_path = compression_config
        .output_dir
        .join(format!("{}.tar.gz", &random_string));

    create_tar(source_dir, &tar_path)?;

    let tar_file = Path::new(&tar_path);
    let tar_sha256sum = try_digest(tar_file)
        .with_context(|| format!("Failed to calculate sha256sum of {}", tar_path.display()))?;
    let tar_metadata = fs::metadata(tar_file)
        .with_context(|| format!("Failed to read size of {}", tar_path.display()))?;

    compress_to_gz(&tar_path, &gz_path)?;

    fs::remove_file(&tar_path)?;

    let gz_file = Path::new(&gz_path);
    let gz_sha256sum = try_digest(gz_file)
        .with_context(|| format!("Failed to calculate sha256sum of {}", gz_path.display()))?;
    let gz_metadata = fs::metadata(gz_file)
        .with_context(|| format!("Failed to read size of {}", gz_path.display()))?;

    let formatted_gz_path = compression_config.output_dir.join(&gz_sha256sum);
    fs::rename(&gz_path, &formatted_gz_path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            gz_path.display(),
            formatted_gz_path.display()
        )
    })?;
    // fs::remove_file(&gz_path)?;

    Ok(LayerCompressionResult::new(
        tar_sha256sum,
        tar_metadata.len(),
        gz_sha256sum,
        gz_metadata.len(),
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::compression::layer_compression_config::LayerCompressionConfig;

    use super::compress;

    #[test]
    fn test_compression() {
        let tmp_dir = tempdir().unwrap();
        let layer_dir = tmp_dir.path().to_path_buf();

        let layer_file = layer_dir.join("file.txt");
        fs::write(&layer_file, "Hello, world!").unwrap();

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compression_config = LayerCompressionConfig::new(layer_dir, output_dir);

        let compression_result1 = compress(&compression_config).unwrap();

        let tmp_dir = tempdir().unwrap();
        let layer_dir = tmp_dir.path().to_path_buf();

        let layer_file = layer_dir.join("file.txt");
        fs::write(&layer_file, "Hello, world!").unwrap();

        let tmp_dir = tempdir().unwrap();
        let output_dir = tmp_dir.path().to_path_buf();
        let compression_config = LayerCompressionConfig::new(layer_dir, output_dir);

        let compression_result2 = compress(&compression_config).unwrap();

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
}
