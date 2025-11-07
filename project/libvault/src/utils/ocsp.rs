use better_default::Default;
use openssl::x509::X509;

#[repr(u32)]
#[derive(Debug)]
pub enum FailureMode {
    OcspFailOpenNotSet = 0,
    FailOpenTrue = 1,
    FailOpenFalse = 2,
}

#[derive(Default, Debug)]
pub struct OcspConfig {
    pub enable: bool,
    pub extra_ca: Vec<X509>,
    pub servers_override: Vec<String>,
    #[default(FailureMode::OcspFailOpenNotSet)]
    pub failure_mode: FailureMode,
    pub query_all_servers: bool,
}
