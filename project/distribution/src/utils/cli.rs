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
}
