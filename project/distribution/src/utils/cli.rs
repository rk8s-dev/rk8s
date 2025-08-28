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
    #[arg(short, long, env = "OCI_REGISTRY_STORAGE", default_value = "FILESYSTEM")]
    pub(crate) storage: String,

    /// Registry root path
    #[arg(long, env = "OCI_REGISTRY_ROOTDIR", default_value = "/var/lib/oci-registry")]
    pub(crate) root: String,

    /// Registry url
    #[arg(long, env = "OCI_REGISTRY_PUBLIC_URL", default_value = "http://127.0.0.1:8968")]
    pub(crate) url: String,

    /// The database URL to connect to
    #[arg(long, env = "DATABASE_URL", default_value = "db/registry.db")]
    pub(crate) database_url: String,
}
