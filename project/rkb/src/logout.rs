use crate::login::config::LoginConfig;
use crate::utils::cli::assert_not_sudo;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    /// URL of the distribution server
    url: Option<String>,
}

pub fn logout(args: LogoutArgs) -> anyhow::Result<()> {
    assert_not_sudo("logout")?;
    match args.url {
        Some(url) => LoginConfig::logout(&url)?,
        None => {
            let config = LoginConfig::load()?;
            let entry = config.single_entry()?;
            LoginConfig::logout(&entry.url)?;
        }
    }
    println!("Successfully logged out!");
    Ok(())
}
