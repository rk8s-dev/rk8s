use std::path::Path;

pub enum MediaType {
    Tar,
    TarGzip,
    Other,
}

impl MediaType {
    pub fn unpack(&self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
        let src = src.as_ref();
        let dst = dst.as_ref();

        match self {
            MediaType::Tar => {
                std::fs::create_dir_all(dst)?;
                let tar_gz = std::fs::File::open(src)?;
                let mut archive = tar::Archive::new(tar_gz);
                archive.unpack(dst)?;
            }
            MediaType::TarGzip => {
                std::fs::create_dir_all(dst)?;
                let tar_gz = std::fs::File::open(src)?;
                let decompressor = flate2::read::GzDecoder::new(tar_gz);
                let mut archive = tar::Archive::new(decompressor);
                archive.unpack(dst)?;
            }
            MediaType::Other => {
                std::fs::copy(src, dst)?;
            }
        }
        Ok(())
    }
}

pub fn get_media_type(media_type: &str) -> MediaType {
    // OCI uses `+gzip` (e.g. application/vnd.oci.image.layer.v1.tar+gzip),
    // Docker uses `.gzip` (e.g. application/vnd.docker.image.rootfs.diff.tar.gzip).
    if media_type.ends_with("tar+gzip") || media_type.ends_with("tar.gzip") {
        return MediaType::TarGzip;
    }
    if media_type.ends_with("tar") {
        return MediaType::Tar;
    }
    MediaType::Other
}
