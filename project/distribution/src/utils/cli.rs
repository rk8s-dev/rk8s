use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Registry listening host
    #[arg(long, env = "OCI_REGISTRY_URL", default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Registry listening port
    #[arg(short, long, env = "OCI_REGISTRY_PORT", default_value_t = 8968)]
    pub(crate) port: u16,

    /// Storage backend type
    #[arg(
        short,
        long,
        env = "OCI_REGISTRY_STORAGE",
        default_value = "FILESYSTEM"
    )]
    pub(crate) storage: String,

    /// Registry root path
    #[arg(
        long,
        env = "OCI_REGISTRY_ROOTDIR",
        default_value = "/var/lib/oci-registry"
    )]
    pub(crate) root: String,

    /// Registry url
    #[arg(
        long,
        env = "OCI_REGISTRY_PUBLIC_URL",
        default_value = "http://127.0.0.1:8968"
    )]
    pub(crate) url: String,

    /// Web App Internal Verify API URL
    #[arg(
        long,
        env = "OCI_REGISTRY_AUTH_API_URL",
        default_value = "http://127.0.0.1:7001/api/internal/verify"
    )]
    pub(crate) auth_api_url: String,

    /// Shared internal token used for Distribution -> Web App verify calls.
    #[arg(long, env = "INTERNAL_VERIFY_TOKEN")]
    pub(crate) internal_verify_token: Option<String>,

    /// Database host
    #[arg(long, env = "POSTGRES_HOST", default_value = "localhost")]
    pub(crate) db_host: String,

    /// Database port
    #[arg(long, env = "POSTGRES_PORT", default_value_t = 5432)]
    pub(crate) db_port: u16,

    /// Database user
    #[arg(long, env = "POSTGRES_USER", default_value = "postgres")]
    pub(crate) db_user: String,

    /// Database name
    #[arg(long, env = "POSTGRES_DB", default_value = "postgres")]
    pub(crate) db_name: String,

    // --- S3 storage backend options ---
    /// S3 bucket name
    #[arg(long, env = "OCI_REGISTRY_S3_BUCKET")]
    pub(crate) s3_bucket: Option<String>,

    /// S3 region
    #[arg(long, env = "OCI_REGISTRY_S3_REGION", default_value = "us-east-1")]
    pub(crate) s3_region: String,

    /// S3 endpoint (required for MinIO/RustFS)
    #[arg(long, env = "OCI_REGISTRY_S3_ENDPOINT")]
    pub(crate) s3_endpoint: Option<String>,

    /// S3 access key ID
    #[arg(long, env = "OCI_REGISTRY_S3_ACCESS_KEY_ID")]
    pub(crate) s3_access_key_id: Option<String>,

    /// S3 secret access key
    #[arg(long, env = "OCI_REGISTRY_S3_SECRET_ACCESS_KEY")]
    pub(crate) s3_secret_access_key: Option<String>,

    /// Use path-style addressing (required for MinIO/RustFS)
    #[arg(long, env = "OCI_REGISTRY_S3_PATH_STYLE", default_value_t = false)]
    pub(crate) s3_path_style: bool,

    /// Allow plain HTTP (only for dev/testing)
    #[arg(long, env = "OCI_REGISTRY_S3_ALLOW_HTTP", default_value_t = false)]
    pub(crate) s3_allow_http: bool,
}
