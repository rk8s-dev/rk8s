#[allow(unused_variables)]
#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub storge_typ: String,
    pub root_dir: String,
    pub registry_url: String,
    pub db_url: String,
    pub password_salt: String,
    pub jwt_secret: String,
    pub jwt_lifetime_secs: i64,
}
