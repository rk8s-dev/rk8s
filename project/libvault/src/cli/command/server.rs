use std::{
    default::Default,
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use actix_web::{
    middleware::{self, from_fn},
    web, App, HttpResponse, HttpServer,
};
use anyhow::format_err;
use clap::Parser;
use derive_more::Deref;
use openssl::{
    ssl::{SslAcceptor, SslFiletype, SslMethod, SslOptions, SslVerifyMode, SslVersion},
    x509::{store::X509StoreBuilder, verify::X509VerifyFlags, X509},
};
use sysexits::ExitCode;

use crate::{
    cli::{command, config},
    errors::RvError,
    http,
    metrics::{manager::MetricsManager, middleware::metrics_midleware},
    storage, RustyVault, EXIT_CODE_INSUFFICIENT_PARAMS, EXIT_CODE_LOAD_CONFIG_FAILURE, EXIT_CODE_OK,
};

pub const WORK_DIR_PATH_DEFAULT: &str = "/tmp/rusty_vault";

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"This command starts a RustyVault server that responds to API requests. By default,
RustyVault will start in a "sealed" state. The RustyVault cluster must be initialized
before use, usually by the "rvault operator init" command. Each RustyVault server must
also be unsealed using the "rvault operator unseal" command or the API before the
server can respond to requests.

Start a server with a configuration file:

  $ rvault server --config=/etc/rvault/config.hcl"#
)]
pub struct Server {
    #[deref]
    #[command(flatten, next_help_heading = "Command Options")]
    command_options: command::CommandOptions,
}

impl Server {
    #[inline]
    pub fn execute(&mut self) -> ExitCode {
        if let Some(config_path) = &self.config {
            return match self.main(config_path) {
                Ok(_) => EXIT_CODE_OK,
                Err(e) => {
                    println!("server error: {e:?}");
                    std::process::exit(EXIT_CODE_LOAD_CONFIG_FAILURE as i32);
                }
            };
        }

        EXIT_CODE_INSUFFICIENT_PARAMS
    }

    pub fn main(&self, config_path: &PathBuf) -> Result<(), RvError> {
        let config = config::load_config(&config_path.to_string_lossy())?;

        if config.storage.len() != 1 {
            return Err(RvError::ErrConfigStorageNotFound);
        }

        if config.listener.len() != 1 {
            return Err(RvError::ErrConfigListenerNotFound);
        }

        env::set_var("RUST_LOG", config.log_level.as_str());
        env_logger::init();

        let (_, storage) = config.storage.iter().next().unwrap();
        let (_, listener) = config.listener.iter().next().unwrap();

        let listener = listener.clone();

        let mut work_dir = WORK_DIR_PATH_DEFAULT.to_string();
        if !config.work_dir.is_empty() {
            work_dir.clone_from(&config.work_dir);
        }

        if !Path::new(work_dir.as_str()).exists() {
            log::info!("create work_dir: {work_dir}");
            fs::create_dir_all(work_dir.as_str())?;
        }

        #[cfg(not(windows))]
        if config.daemon {
            // start daemon
            let log_path = format!("{work_dir}/rusty_vault.log");
            let mut pid_path = config.pid_file.clone();
            if !config.pid_file.starts_with('/') {
                pid_path = work_dir.clone() + pid_path.as_str();
            }

            let mut user = "onbody".to_owned();
            if !config.daemon_user.is_empty() {
                user.clone_from(&config.daemon_user);
            }

            let mut group = "onbody".to_owned();
            if !config.daemon_group.is_empty() {
                group.clone_from(&config.daemon_group);
            }

            let log_file = std::fs::OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .truncate(false)
                .open(log_path)
                .unwrap();

            let daemonize = daemonize::Daemonize::new()
                .working_directory(work_dir.as_str())
                .user(user.as_str())
                .group(group.as_str())
                .umask(0o027)
                .stdout(log_file.try_clone().unwrap())
                .stderr(log_file)
                .pid_file(pid_path.clone())
                .chown_pid_file(true)
                .privileged_action(|| log::info!("Start rusty_vault server daemon"));

            match daemonize.start() {
                Ok(_) => {
                    let pid = std::fs::read_to_string(pid_path)?;
                    log::info!("The rusty_vault server daemon process started successfully, pid is {pid}");
                    log::debug!("run user: {user}, group: {group}");
                }
                Err(e) => log::error!("Error, {e}"),
            }
        }

        log::debug!("config_path: {}, work_dir_path: {}", config_path.to_string_lossy(), work_dir.as_str());

        let server = actix_rt::System::new();

        let backend = storage::new_backend(storage.stype.as_str(), &storage.config).unwrap();

        let metrics_manager = Arc::new(RwLock::new(MetricsManager::new(config.collection_interval)));
        let system_metrics = metrics_manager.read().unwrap().system_metrics.clone();

        let rvault = RustyVault::new(backend, Some(&config))?;
        let core = rvault.core.load().clone();

        let mut http_server = HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(from_fn(metrics_midleware))
                .app_data(web::Data::new(core.clone()))
                .app_data(web::Data::new(metrics_manager.clone()))
                .configure(http::init_service)
                .default_service(web::to(HttpResponse::NotFound))
        })
        .on_connect(http::request_on_connect_handler);

        log::info!(
            "start listen, addr: {}, tls_disable: {}, tls_disable_client_certs: {}",
            listener.address,
            listener.tls_disable,
            listener.tls_disable_client_certs
        );

        if listener.tls_disable {
            http_server = http_server.bind(listener.address)?;
        } else {
            let cert_file: &Path = Path::new(&listener.tls_cert_file);
            let key_file: &Path = Path::new(&listener.tls_key_file);

            let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
            builder
                .set_private_key_file(key_file, SslFiletype::PEM)
                .map_err(|err| format_err!("unable to read proxy key {} - {}", key_file.display(), err))?;
            builder
                .set_certificate_chain_file(cert_file)
                .map_err(|err| format_err!("unable to read proxy cert {} - {}", cert_file.display(), err))?;
            builder.check_private_key()?;

            builder.set_min_proto_version(Some(listener.tls_min_version))?;
            builder.set_max_proto_version(Some(listener.tls_max_version))?;

            log::info!("tls_cipher_suites: {}", listener.tls_cipher_suites);
            builder.set_cipher_list(&listener.tls_cipher_suites)?;

            if listener.tls_max_version == SslVersion::TLS1_3 {
                builder.clear_options(SslOptions::NO_TLSV1_3);
                builder
                    .set_ciphersuites("TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256")?;
            }

            if !listener.tls_disable_client_certs {
                builder.set_verify_callback(SslVerifyMode::PEER, |_, _| true);
            }

            if listener.tls_require_and_verify_client_cert {
                builder.set_verify_callback(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT, move |p, _x| p);

                if !listener.tls_client_ca_file.is_empty() {
                    let mut store = X509StoreBuilder::new()?;

                    let mut client_ca_file = File::open(&listener.tls_client_ca_file)?;
                    let mut client_ca_file_bytes = Vec::new();
                    client_ca_file.read_to_end(&mut client_ca_file_bytes)?;
                    let client_ca_x509s = X509::stack_from_pem(&client_ca_file_bytes)?;

                    client_ca_x509s.iter().try_for_each(|cert| store.add_cert(cert.clone()))?;

                    store.set_flags(X509VerifyFlags::PARTIAL_CHAIN)?;
                    builder.set_verify_cert_store(store.build())?;
                }
            }

            http_server = http_server.bind_openssl(listener.address, builder)?;
        }

        log::info!("rusty_vault server starts, waiting for request...");

        server.block_on(async {
            tokio::spawn(async {
                system_metrics.start_collecting().await;
            });
            http_server.run().await
        })?;
        let _ = server.run();

        Ok(())
    }
}
