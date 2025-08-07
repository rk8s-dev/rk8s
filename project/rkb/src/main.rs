pub mod build;
pub mod chroot;
pub mod compression;
pub mod oci_spec;
pub mod overlayfs;
pub mod parse;
pub mod registry;

use clap::Parser;
use parse::build_args::BuildArgs;

fn main() {
    let build_args = BuildArgs::parse();
    match parse::execute(&build_args) {
        Ok(_) => {}
        Err(build_error) => {
            eprintln!("{build_error:?}");
            std::process::exit(1);
        }
    }
}
