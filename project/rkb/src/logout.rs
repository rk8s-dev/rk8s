use crate::config::auth::AuthConfig;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    /// URL of the distribution server
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
