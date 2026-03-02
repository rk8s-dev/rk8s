// Warning: Due to the main.rs file use the way like `aardvark-dns::commands` to import the module.
// This "self-imports" does cause buck2's compile error, since the aardvark-dns crate is patched from it's main repository
// and rkforge will not use the main.rs. The main.rs's code is commented for now.

// use std::env;
// use std::str::FromStr;

// use clap::{Parser, Subcommand};

// use aardvark-dns::commands::{run, version};
// use log::Level;
// use syslog::{BasicLogger, Facility, Formatter3164};

// #[derive(Parser, Debug)]
// #[clap(version = env!("CARGO_PKG_VERSION"))]
// struct Opts {
//     /// Path to configuration directory
//     #[clap(short, long)]
//     config: Option<String>,
//     /// Host port for aardvark servers, defaults to 5533
//     #[clap(short, long)]
//     port: Option<u16>,
//     /// Filters search domain for backward compatiblity with dnsname/dnsmasq
//     #[clap(short, long)]
//     filter_search_domain: Option<String>,
//     /// Aardvark-dns trig command
//     #[clap(subcommand)]
//     subcmd: SubCommand,
// }

// #[derive(Subcommand, Debug)]
// enum SubCommand {
//     /// Runs the aardvark dns server with the specified configuration directory.
//     Run(run::Run),
//     /// Display info about aardvark.
//     Version(version::Version),
// }

fn main() {
    // let formatter = Formatter3164 {
    //     facility: Facility::LOG_USER,
    //     hostname: None,
    //     process: "aardvark-dns".into(),
    //     pid: 0,
    // };

    // let log_level = match env::var("RUST_LOG") {
    //     Ok(val) => match Level::from_str(&val) {
    //         Ok(level) => level,
    //         Err(e) => {
    //             eprintln!("failed to parse RUST_LOG level: {e}");
    //             Level::Info
    //         }
    //     },
    //     // if env is not set default to info
    //     Err(_) => Level::Info,
    // };

    // // On error do nothing, running on system without syslog is fine and we should not clutter
    // // logs with meaningless errors, https://github.com/containers/podman/issues/19809.
    // if let Ok(logger) = syslog::unix(formatter) {
    //     if let Err(e) = log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
    //         .map(|()| log::set_max_level(log_level.to_level_filter()))
    //     {
    //         eprintln!("failed to initialize syslog logger: {e}")
    //     };
    // }

    // let opts = Opts::parse();

    // let dir = opts.config.unwrap_or_else(|| String::from("/dev/stdin"));
    // let port = opts.port.unwrap_or(5533_u16);
    // let filter_search_domain = opts
    //     .filter_search_domain
    //     .unwrap_or_else(|| String::from(".dns.podman"));
    // let result = match opts.subcmd {
    //     SubCommand::Run(run) => run.exec(dir, port, filter_search_domain),
    //     SubCommand::Version(version) => {
    //         version.exec();
    //         Ok(())
    //     }
    // };

    // match result {
    //     Ok(_) => {}
    //     Err(err) => {
    //         eprintln!("{err}");
    //         std::process::exit(1);
    //     }
    // }
}

// #[cfg(test)]
mod test;
