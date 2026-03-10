use crate::config::auth::AuthConfig;
use crate::registry::parse_registry_host_arg;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    /// Registry host in `host[:port]` format.
    #[arg(value_parser = parse_registry_host_arg)]
    url: Option<String>,
}

pub fn logout(args: LogoutArgs) -> anyhow::Result<()> {
    match args.url {
        Some(url) => AuthConfig::logout(&url)?,
        None => {
            let config = AuthConfig::load()?;
            let entry = config.single_entry()?;
            AuthConfig::logout(&entry.url)?;
        }
    }
    println!("Successfully logged out!");
    Ok(())
}
