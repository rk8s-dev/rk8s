use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Registry listening host
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Registry listening port
    #[arg(short, long, default_value_t = 8968)]
    pub(crate) port: u16,

    /// Storage backend type
    #[arg(short, long, default_value = "FILESYSTEM")]
    pub(crate) storage: String,

    /// Registry root path
    #[arg(long, default_value = "/var/lib/oci-registry")]
    pub(crate) root: String,
}