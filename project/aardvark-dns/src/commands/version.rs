use clap::Parser;
use std::fmt;

#[derive(Parser, Debug)]
pub struct Version {}

#[derive(Debug)]
struct Info {
    version: &'static str,
    commit: &'static str,
    build_time: &'static str,
    target: &'static str,
}

// since we do not need a json library here we just create the json output manually
impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{
  \"version\": \"{}\",
  \"commit\": \"{}\",
  \"build_time\": \"{}\",
  \"target\": \"{}\"
}}",
            self.version, self.commit, self.build_time, self.target
        )
    }
}

impl Version {
    pub fn exec(&self) {
        let info = Info {
            version: env!("CARGO_PKG_VERSION"),
            commit: env!("GIT_COMMIT"),
            build_time: env!("BUILD_TIMESTAMP"),
            target: env!("BUILD_TARGET"),
        };
        println!("{info}");
    }
}
