use clap::Parser;
use derive_more::Deref;
use ureq::json;

use crate::{
    api::sys::MountOutput,
    cli::command::{self, format::table_data_add_header, CommandExecutor},
    errors::RvError,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Lists the enabled secret engines on the RustyVault server. This command also outputs
information about the enabled path including configured TTLs and human-friendly
descriptions. A TTL of "system" indicates that the system default is in use.

List all enabled secrets engines:

    $ rvault secrets list

List all enabled secrets engines with detailed output:

    $ rvault secrets list -detailed"#
)]
pub struct List {
    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for List {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        match sys.list_mounts() {
            Ok(ret) => {
                if ret.response_status == 200 && ret.response_data.is_some() {
                    let value = ret.response_data.as_ref().unwrap();
                    if !value.is_object() {
                        return Err(RvError::ErrResponseDataInvalid);
                    }
                    let mut out = json!([]);
                    let out_arr = &mut out.as_array_mut().unwrap();
                    let paths = value.as_object().unwrap();
                    for (path, mount) in paths.iter() {
                        let mount_output: MountOutput = serde_json::from_value(mount.clone())?;
                        out_arr.push(json!([
                            path,
                            &mount_output.logical_type,
                            &mount_output.accessor,
                            &mount_output.description,
                            &mount_output.plugin_version
                        ]));
                    }

                    let data = if self.output.is_format_table() {
                        &table_data_add_header(&out, &["Path", "Type", "Accessor", "Description", "Version"])?
                    } else {
                        value
                    };
                    self.output.print_value(data, false)?;
                } else if ret.response_status == 404 {
                    println!("No value found");
                    return Err(RvError::ErrRequestNoData);
                } else {
                    ret.print_debug_info();
                }
            }
            Err(e) => eprintln!("Error sealing: {e}"),
        }
        Ok(())
    }
}
