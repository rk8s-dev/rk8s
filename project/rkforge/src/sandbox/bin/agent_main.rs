use rkforge::sandbox::agent::run_binary;
use tracing_subscriber::prelude::*;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    run_binary()
}
