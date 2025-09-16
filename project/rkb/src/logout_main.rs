use crate::login_main::LoginConfig;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    url: Option<String>,
}

pub fn logout(args: LogoutArgs) -> anyhow::Result<()> {
    match args.url {
        Some(url) => LoginConfig::logout(&url)?,
        None => {
            let config = LoginConfig::load()?;
            match config.single_entry() {
                Ok(entry) => LoginConfig::logout(&entry.url)?,
                Err(_) => {
                    println!("There are several entries, please select a url to logout.");
                    return Ok(());
                }
            }
        }
    }
    println!("Successfully logged out!");
    Ok(())
}
