use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::path::PathBuf;

use crate::chunk::layout::{DEFAULT_BLOCK_SIZE, DEFAULT_CHUNK_SIZE};

pub const DEFAULT_DATA_DIR: &str = "./data";
pub const DEFAULT_META_URL: &str = "sqlite::memory:";
pub const DEFAULT_S3_PART_SIZE: usize = 16 * 1024 * 1024;
pub const DEFAULT_S3_MAX_CONCURRENCY: usize = 8;

#[derive(Parser)]
#[command(name = "slayerfs", version, about = "SlayerFS FUSE CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Mount SlayerFS via FUSE.
    #[command(
        after_help = "Examples:\n  slayerfs mount --config examples/mount-config.local.yaml\n  slayerfs mount --config examples/mount-config.s3.yaml\n  slayerfs mount --config examples/mount-config.local.yaml /mnt/slayer\n  slayerfs mount --config examples/mount-config.s3.yaml --s3-bucket override-bucket"
    )]
    Mount(Box<MountArgs>),

    /// Talk to a mounted SlayerFS instance and run orphan gc.
    Gc(GcArgs),

    /// Talk to a mounted SlayerFS instance and print mount information.
    Info(InfoArgs),
}

#[derive(Args, Debug, Clone)]
pub struct MountArgs {
    /// YAML config file path.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Directory to mount the filesystem.
    #[arg(value_name = "MOUNT_POINT")]
    pub mount_point: Option<PathBuf>,

    /// Data storage backend type.
    #[arg(long, value_enum)]
    pub data_backend: Option<DataBackendKind>,

    /// Local directory used as object storage backend (only for localfs backend).
    #[arg(long, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    /// S3 bucket name (only for s3 backend).
    #[arg(long, value_name = "BUCKET")]
    pub s3_bucket: Option<String>,

    /// S3-compatible endpoint URL (only for s3 backend).
    #[arg(long, value_name = "URL")]
    pub s3_endpoint: Option<String>,

    /// S3 region (optional, for s3 backend).
    #[arg(long, value_name = "REGION")]
    pub s3_region: Option<String>,

    /// S3 part size in bytes for multipart upload (only for s3 backend).
    #[arg(long)]
    pub s3_part_size: Option<usize>,

    /// S3 maximum concurrent multipart upload parts (only for s3 backend).
    #[arg(long)]
    pub s3_max_concurrency: Option<usize>,

    /// Force path-style S3 access (required for MinIO, localstack, etc.).
    #[arg(long)]
    pub s3_force_path_style: Option<bool>,

    /// Metadata backend (sqlx, etcd or redis).
    #[arg(long, value_enum)]
    pub meta_backend: Option<MetaBackendKind>,

    /// Metadata backend URL (sqlx or redis, e.g. sqlite::memory:, postgres://... or redis://...).
    #[arg(long, value_name = "URL")]
    pub meta_url: Option<String>,

    /// Etcd endpoint URLs (comma-separated).
    #[arg(long, value_name = "URLS", value_delimiter = ',')]
    pub meta_etcd_urls: Option<Vec<String>>,

    /// Chunk size in bytes.
    #[arg(long)]
    pub chunk_size: Option<u64>,

    /// Block size in bytes.
    #[arg(long)]
    pub block_size: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct GcArgs {
    /// Optional mount point used to locate the target instance.
    #[arg(value_name = "MOUNT_POINT")]
    pub mount_point: Option<PathBuf>,

    /// Scan only; do not delete orphan data.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Args, Debug, Clone)]
pub struct InfoArgs {
    /// Optional mount point used to locate the target instance.
    #[arg(value_name = "MOUNT_POINT")]
    pub mount_point: Option<PathBuf>,
}

#[derive(ValueEnum, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DataBackendKind {
    LocalFs,
    S3,
}

#[derive(ValueEnum, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum MetaBackendKind {
    Sqlx,
    Etcd,
    Redis,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MountFileConfig {
    pub mount_point: Option<PathBuf>,
    pub data: Option<DataFileConfig>,
    pub meta: Option<MetaFileConfig>,
    pub layout: Option<LayoutFileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DataFileConfig {
    pub backend: Option<DataBackendKind>,
    pub localfs: Option<LocalFsFileConfig>,
    pub s3: Option<S3FileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocalFsFileConfig {
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct S3FileConfig {
    pub bucket: Option<String>,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub part_size: Option<usize>,
    pub max_concurrency: Option<usize>,
    pub force_path_style: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MetaFileConfig {
    pub backend: Option<MetaBackendKind>,
    pub sqlx: Option<UrlBackedMetaFileConfig>,
    pub redis: Option<UrlBackedMetaFileConfig>,
    pub etcd: Option<EtcdMetaFileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UrlBackedMetaFileConfig {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EtcdMetaFileConfig {
    pub urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LayoutFileConfig {
    pub chunk_size: Option<u64>,
    pub block_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct MountConfig {
    pub mount_point: PathBuf,
    pub data_backend: DataBackendKind,
    pub data_dir: PathBuf,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_region: Option<String>,
    pub s3_part_size: usize,
    pub s3_max_concurrency: usize,
    pub s3_force_path_style: bool,
    pub meta_backend: MetaBackendKind,
    pub meta_url: String,
    pub meta_etcd_urls: Vec<String>,
    pub chunk_size: u64,
    pub block_size: u32,
}

impl MountConfig {
    pub fn from_sources(args: MountArgs) -> anyhow::Result<Self> {
        let file_cfg = match args.config.as_ref() {
            Some(path) => {
                let content = std::fs::read_to_string(path)?;
                serde_yaml::from_str::<MountFileConfig>(&content)?
            }
            None => MountFileConfig::default(),
        };

        let data_cfg = file_cfg.data.unwrap_or_default();
        let localfs_cfg = data_cfg.localfs.unwrap_or_default();
        let s3_cfg = data_cfg.s3.unwrap_or_default();
        let meta_cfg = file_cfg.meta.unwrap_or_default();
        let sqlx_cfg = meta_cfg.sqlx.unwrap_or_default();
        let redis_cfg = meta_cfg.redis.unwrap_or_default();
        let etcd_cfg = meta_cfg.etcd.unwrap_or_default();
        let layout_cfg = file_cfg.layout.unwrap_or_default();

        let mount_point = args.mount_point.or(file_cfg.mount_point).ok_or_else(|| {
            anyhow::anyhow!("mount point is required (positional arg or config.mount_point)")
        })?;

        let meta_backend = args
            .meta_backend
            .or(meta_cfg.backend)
            .unwrap_or(MetaBackendKind::Sqlx);

        let meta_url_from_file = match meta_backend {
            MetaBackendKind::Sqlx => sqlx_cfg.url,
            MetaBackendKind::Redis => redis_cfg.url,
            MetaBackendKind::Etcd => None,
        };

        Ok(Self {
            mount_point,
            data_backend: args
                .data_backend
                .or(data_cfg.backend)
                .unwrap_or(DataBackendKind::LocalFs),
            data_dir: args
                .data_dir
                .or(localfs_cfg.data_dir)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR)),
            s3_bucket: args.s3_bucket.or(s3_cfg.bucket),
            s3_endpoint: args.s3_endpoint.or(s3_cfg.endpoint),
            s3_region: args.s3_region.or(s3_cfg.region),
            s3_part_size: args
                .s3_part_size
                .or(s3_cfg.part_size)
                .unwrap_or(DEFAULT_S3_PART_SIZE),
            s3_max_concurrency: args
                .s3_max_concurrency
                .or(s3_cfg.max_concurrency)
                .unwrap_or(DEFAULT_S3_MAX_CONCURRENCY),
            s3_force_path_style: args
                .s3_force_path_style
                .or(s3_cfg.force_path_style)
                .unwrap_or(false),
            meta_backend,
            meta_url: args
                .meta_url
                .or(meta_url_from_file)
                .unwrap_or_else(|| DEFAULT_META_URL.to_string()),
            meta_etcd_urls: args.meta_etcd_urls.or(etcd_cfg.urls).unwrap_or_default(),
            chunk_size: args
                .chunk_size
                .or(layout_cfg.chunk_size)
                .unwrap_or(DEFAULT_CHUNK_SIZE),
            block_size: args
                .block_size
                .or(layout_cfg.block_size)
                .unwrap_or(DEFAULT_BLOCK_SIZE),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn info_subcommand_parses_mount_point() {
        let cli = Cli::parse_from(["slayerfs", "info", "/mnt/slayer"]);

        match cli.cmd {
            Command::Info(args) => {
                assert_eq!(args.mount_point, Some(PathBuf::from("/mnt/slayer")));
            }
            other => panic!("expected info command, got {other:?}"),
        }
    }
}
