use clap::{ArgMatches, Command};
use xline_client::{Client, error::Result};

use crate::utils::printer::Printer;

/// Definition of `list` command
pub(super) fn command() -> Command {
    Command::new("list").about("List all roles")
}

/// Execute the command
pub(super) async fn execute(client: &mut Client, _matches: &ArgMatches) -> Result<()> {
    let resp = client.auth_client().role_list().await?;
    resp.print();

    Ok(())
}
