use clap::Parser;
use parse::build_args::BuildArgs;
use rkb::parse;

fn main() {
    let build_args = BuildArgs::parse();
    match parse::execute(&build_args) {
        Ok(_) => {}
        Err(build_error) => {
            eprintln!("{:?}", build_error);
            std::process::exit(1);
        }
    }
}
