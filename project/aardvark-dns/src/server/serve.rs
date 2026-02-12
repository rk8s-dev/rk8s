use crate::backend::DNSBackend;
use crate::config::constants::AARDVARK_PID_FILE;
use crate::config::parse_configs;
use crate::dns::coredns::CoreDns;
use crate::dns::coredns::DNS_PORT;
use crate::error::AardvarkError;
use crate::error::AardvarkErrorList;
use crate::error::AardvarkResult;
use crate::error::AardvarkWrap;
use arc_swap::ArcSwap;
use log::{debug, error, info};
use nix::unistd::{self, dup2_stderr, dup2_stdin, dup2_stdout};
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use tokio::net::{TcpListener, UdpSocket};
use tokio::signal::unix::{signal, SignalKind};
use tokio::task::JoinHandle;

use futures::StreamExt;
use inotify::{EventStream, Inotify, WatchMask};
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::process;

const RESOLV_CONF: &str = "/etc/resolv.conf";

type ThreadHandleMap<Ip> =
    HashMap<(String, Ip), (flume::Sender<()>, JoinHandle<AardvarkResult<()>>)>;

pub fn create_pid(config_path: &str) -> AardvarkResult<()> {
    // before serving write its pid to _config_path so other process can notify
    // aardvark of data change.
    let path = Path::new(config_path).join(AARDVARK_PID_FILE);
    let mut pid_file = match File::create(path) {
        Err(err) => {
            return Err(AardvarkError::msg(format!(
                "Unable to get process pid: {err}"
            )));
        }
        Ok(file) => file,
    };

    let server_pid = process::id().to_string();
    if let Err(err) = pid_file.write_all(server_pid.as_bytes()) {
        return Err(AardvarkError::msg(format!(
            "Unable to write pid to file: {err}"
        )));
    }

    Ok(())
}

#[tokio::main]
pub async fn serve(
    config_path: &str,
    port: u16,
    filter_search_domain: &str,
    ready: OwnedFd,
) -> AardvarkResult<()> {
    let mut signals = signal(SignalKind::hangup())?;
    let no_proxy: bool = env::var("AARDVARK_NO_PROXY").is_ok();

    let mut handles_v4 = HashMap::new();
    let mut handles_v6 = HashMap::new();
    let nameservers = Arc::new(Mutex::new(Vec::new()));

    read_config_and_spawn(
        config_path,
        port,
        filter_search_domain,
        &mut handles_v4,
        &mut handles_v6,
        nameservers.clone(),
        no_proxy,
    )
    .await?;
    // We are ready now, this is far from perfect we should at least wait for the first bind
    // to work but this is not really possible with the current code flow and needs more changes.
    daemonize()?;
    let msg: [u8; 1] = [b'1'];
    unistd::write(&ready, &msg)?;
    drop(ready);

    // Setup inotify to monitor resolv.conf
    let mut event_stream = get_inotify_event_stream();
    loop {
        tokio::select! {
            // Block until we receive a SIGHUP.
            _= signals.recv()=>{
                debug!("Received SIGHUP");
                if let Err(e) = read_config_and_spawn(
                    config_path,
                    port,
                    filter_search_domain,
                    &mut handles_v4,
                    &mut handles_v6,
                    nameservers.clone(),
                    no_proxy,
                )
                .await
                {
                    // do not exit here, we just keep running even if something failed
                    error!("{e}");
                };
            }
            // Block until resolv.conf is changed, monitored via inotify. Then reload nameservers
            _ = event_stream.as_mut().unwrap().next(), if event_stream.is_some() => {
                let upstream_resolvers = match get_upstream_resolvers() {
                    Ok(ns) => ns,
                    Err(err) => {
                        error!("Failed to reload nameservers on change: {err}");
                        continue;
                    }
                };
                match nameservers.lock() {
                    Ok(mut ns) => *ns = upstream_resolvers,
                    Err(err) => {
                        error!("Failed to reload nameservers, could not obtain lock: {err}");
                    }
                }
            }
        }
    }
}
/// # Ensure the expected DNS server threads are running
///
/// Stop threads corresponding to listen IPs no longer in the configuration and start threads
/// corresponding to listen IPs that were added.
async fn stop_and_start_threads<Ip>(
    port: u16,
    backend: &'static ArcSwap<DNSBackend>,
    listen_ips: HashMap<String, Vec<Ip>>,
    thread_handles: &mut ThreadHandleMap<Ip>,
    no_proxy: bool,
    nameservers: Arc<Mutex<Vec<SocketAddr>>>,
) -> AardvarkResult<()>
where
    Ip: Eq + Hash + Copy + Into<IpAddr> + Send + 'static,
{
    let mut expected_threads = HashSet::new();
    for (network_name, listen_ip_list) in listen_ips {
        for ip in listen_ip_list {
            expected_threads.insert((network_name.clone(), ip));
        }
    }

    // First we shut down any old threads that should no longer be running.  This should be
    // done before starting new ones in case a listen IP was moved from being under one network
    // name to another.
    let to_shut_down: Vec<_> = thread_handles
        .keys()
        .filter(|k| !expected_threads.contains(k))
        .cloned()
        .collect();
    stop_threads(thread_handles, Some(to_shut_down)).await;

    // Then we start any new threads.
    let to_start: Vec<_> = expected_threads
        .iter()
        .filter(|k| !thread_handles.contains_key(*k))
        .cloned()
        .collect();

    let mut errors = AardvarkErrorList::new();

    for (network_name, ip) in to_start {
        let (shutdown_tx, shutdown_rx) = flume::bounded(0);
        let network_name_ = network_name.clone();
        let ns = nameservers.clone();
        let addr = SocketAddr::new(ip.into(), port);
        let udp_sock = match UdpSocket::bind(addr).await {
            Ok(s) => s,
            Err(err) => {
                errors.push(AardvarkError::wrap(
                    format!("failed to bind udp listener on {addr}"),
                    err.into(),
                ));
                continue;
            }
        };

        let tcp_sock = match TcpListener::bind(addr).await {
            Ok(s) => s,
            Err(err) => {
                errors.push(AardvarkError::wrap(
                    format!("failed to bind tcp listener on {addr}"),
                    err.into(),
                ));
                continue;
            }
        };

        let handle = tokio::spawn(async move {
            start_dns_server(
                network_name_,
                udp_sock,
                tcp_sock,
                backend,
                shutdown_rx,
                no_proxy,
                ns,
            )
            .await
        });

        thread_handles.insert((network_name, ip), (shutdown_tx, handle));
    }

    if errors.is_empty() {
        return Ok(());
    }

    Err(AardvarkError::List(errors))
}

/// # Stop DNS server threads
///
/// If the `filter` parameter is `Some` only threads in the filter `Vec` will be stopped.
async fn stop_threads<Ip>(
    thread_handles: &mut ThreadHandleMap<Ip>,
    filter: Option<Vec<(String, Ip)>>,
) where
    Ip: Eq + Hash + Copy,
{
    let mut handles = Vec::new();

    let to_shut_down: Vec<_> = filter.unwrap_or_else(|| thread_handles.keys().cloned().collect());

    for key in to_shut_down {
        let (tx, handle) = thread_handles.remove(&key).unwrap();
        handles.push(handle);
        drop(tx);
    }

    for handle in handles {
        match handle.await {
            Ok(res) => {
                // result returned by the future, i.e. that actual
                // result from start_dns_server()
                if let Err(e) = res {
                    error!("Error from dns server: {e}")
                }
            }
            // error from tokio itself
            Err(e) => error!("Error from dns server task: {e}"),
        }
    }
}

async fn start_dns_server(
    name: String,
    udp_socket: UdpSocket,
    tcp_socket: TcpListener,
    backend: &'static ArcSwap<DNSBackend>,
    rx: flume::Receiver<()>,
    no_proxy: bool,
    nameservers: Arc<Mutex<Vec<SocketAddr>>>,
) -> AardvarkResult<()> {
    let server = CoreDns::new(name, backend, rx, no_proxy, nameservers);
    server
        .run(udp_socket, tcp_socket)
        .await
        .wrap("run dns server")
}

async fn read_config_and_spawn(
    config_path: &str,
    port: u16,
    filter_search_domain: &str,
    handles_v4: &mut ThreadHandleMap<Ipv4Addr>,
    handles_v6: &mut ThreadHandleMap<Ipv6Addr>,
    nameservers: Arc<Mutex<Vec<SocketAddr>>>,
    no_proxy: bool,
) -> AardvarkResult<()> {
    let (conf, listen_ip_v4, listen_ip_v6) =
        parse_configs(config_path, filter_search_domain).wrap("unable to parse config")?;

    // We store the `DNSBackend` in an `ArcSwap` so we can replace it when the configuration is
    // reloaded.
    static DNSBACKEND: OnceLock<ArcSwap<DNSBackend>> = OnceLock::new();
    let backend = match DNSBACKEND.get() {
        Some(b) => {
            b.store(Arc::new(conf));
            b
        }
        None => DNSBACKEND.get_or_init(|| ArcSwap::from(Arc::new(conf))),
    };

    debug!("Successfully parsed config");
    debug!("Listen v4 ip {listen_ip_v4:?}");
    debug!("Listen v6 ip {listen_ip_v6:?}");

    // kill server if listen_ip's are empty
    if listen_ip_v4.is_empty() && listen_ip_v6.is_empty() {
        info!("No configuration found stopping the sever");

        let path = Path::new(config_path).join(AARDVARK_PID_FILE);
        if let Err(err) = fs::remove_file(path) {
            error!("failed to remove the pid file: {}", &err);
            process::exit(1);
        }

        // Gracefully stop all server threads first.
        stop_threads(handles_v4, None).await;
        stop_threads(handles_v6, None).await;

        process::exit(0);
    }

    let mut errors = AardvarkErrorList::new();

    // get host nameservers
    let upstream_resolvers = match get_upstream_resolvers() {
        Ok(ns) => ns,
        Err(err) => {
            errors.push(AardvarkError::wrap(
                "failed to get upstream nameservers, dns forwarding will not work",
                err,
            ));
            Vec::new()
        }
    };
    debug!("Using the following upstream servers: {upstream_resolvers:?}");

    {
        // use new scope to only lock for a short time
        *nameservers.lock().expect("lock nameservers") = upstream_resolvers;
    }

    if let Err(err) = stop_and_start_threads(
        port,
        backend,
        listen_ip_v4,
        handles_v4,
        no_proxy,
        nameservers.clone(),
    )
    .await
    {
        errors.push(err)
    };

    if let Err(err) = stop_and_start_threads(
        port,
        backend,
        listen_ip_v6,
        handles_v6,
        no_proxy,
        nameservers,
    )
    .await
    {
        errors.push(err)
    };

    if errors.is_empty() {
        return Ok(());
    }

    Err(AardvarkError::List(errors))
}

// creates new session and put /dev/null on the stdio streams
fn daemonize() -> Result<(), Error> {
    // remove any controlling terminals
    // but don't hardstop if this fails
    let _ = unsafe { libc::setsid() }; // check https://docs.rs/libc

    let dev_null = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .map_err(|e| std::io::Error::new(e.kind(), format!("/dev/null: {e:#}")))?;
    // redirect stdout, stdin and stderr to /dev/null
    let _ = dup2_stdin(&dev_null);
    let _ = dup2_stdout(&dev_null);
    let _ = dup2_stderr(&dev_null);
    Ok(())
}

// read /etc/resolv.conf and return all nameservers
fn get_upstream_resolvers() -> AardvarkResult<Vec<SocketAddr>> {
    let mut f = File::open(RESOLV_CONF).wrap("open resolv.conf")?;
    let mut buf = String::with_capacity(4096);
    f.read_to_string(&mut buf).wrap("read resolv.conf")?;

    parse_resolv_conf(&buf)
}

fn get_inotify_event_stream() -> Option<EventStream<[u8; 1024]>> {
    // Min buffer size is 272 (sizeof(struct inotify_event) + NAME_MAX + 1)
    let buffer = [0; 1024];
    let inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(e) => {
            error!(
                "Failed to initialize inotify. Nameservers will not be updated on resolv.conf change: {e}"
            );
            return None;
        }
    };

    match inotify
        .watches()
        .add(RESOLV_CONF, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO)
    {
        Ok(_) => match inotify.into_event_stream(buffer) {
                Ok(stream) => return Some(stream),
                Err(e) => error!("Failed to stream inotify events. Nameservers will not be updated on resolv.conf change: {e}"),
            },
        Err(e) => error!("Failed to add watch on {RESOLV_CONF}. Nameservers will not be updated on resolv.conf change: {e}"),
    }
    None
}

fn parse_resolv_conf(content: &str) -> AardvarkResult<Vec<SocketAddr>> {
    let mut nameservers = Vec::new();
    for line in content.split('\n') {
        // split of comments
        let line = match line.split_once(['#', ';']) {
            Some((f, _)) => f,
            None => line,
        };
        let mut line_parts = line.split_whitespace();
        match line_parts.next() {
            Some(first) => {
                if first == "nameserver" {
                    if let Some(ip) = line_parts.next() {
                        // split of zone, we do not support the link local zone currently with ipv6 addresses
                        let mut scope = None;
                        let ip = match ip.split_once("%") {
                            Some((ip, scope_name)) => {
                                // allow both interface names or static ids
                                let id = match scope_name.parse() {
                                    Ok(id) => id,
                                    Err(_) => nix::net::if_::if_nametoindex(scope_name)
                                        .wrap("resolve scope id")?,
                                };

                                scope = Some(id);
                                ip
                            }
                            None => ip,
                        };
                        let ip = ip.parse().wrap(ip)?;

                        let addr = match ip {
                            IpAddr::V4(ip) => {
                                if scope.is_some() {
                                    return Err(AardvarkError::msg(
                                        "scope id not supported for ipv4 address",
                                    ));
                                }
                                SocketAddr::V4(SocketAddrV4::new(ip, DNS_PORT))
                            }
                            IpAddr::V6(ip) => SocketAddr::V6(SocketAddrV6::new(
                                ip,
                                DNS_PORT,
                                0,
                                scope.unwrap_or(0),
                            )),
                        };

                        nameservers.push(addr);
                    }
                }
            }
            None => continue,
        }
    }

    // we do not have time to try many nameservers anyway so only use the first three
    nameservers.truncate(3);
    Ok(nameservers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP_1_1_1_1: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 1), DNS_PORT));
    const IP_1_1_1_2: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 2), DNS_PORT));
    const IP_1_1_1_3: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 3), DNS_PORT));

    /// fdfd:733b:dc3:220b::2
    const IP_FDFD_733B_DC3_220B_2: SocketAddr = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::new(0xfdfd, 0x733b, 0xdc3, 0x220b, 0, 0, 0, 2),
        DNS_PORT,
        0,
        0,
    ));

    /// fe80::1%lo
    const IP_FE80_1: SocketAddr = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
        DNS_PORT,
        0,
        1,
    ));

    #[test]
    fn test_parse_resolv_conf() {
        let res = parse_resolv_conf("nameserver 1.1.1.1").expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1]);
    }

    #[test]
    fn test_parse_resolv_conf_multiple() {
        let res = parse_resolv_conf(
            "nameserver 1.1.1.1
nameserver 1.1.1.2
nameserver 1.1.1.3",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1, IP_1_1_1_2, IP_1_1_1_3]);
    }

    #[test]
    fn test_parse_resolv_conf_search_and_options() {
        let res = parse_resolv_conf(
            "nameserver 1.1.1.1
nameserver 1.1.1.2
nameserver 1.1.1.3
search test.podman
options rotate",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1, IP_1_1_1_2, IP_1_1_1_3]);
    }
    #[test]
    fn test_parse_resolv_conf_with_comment() {
        let res = parse_resolv_conf(
            "# mytest
            nameserver 1.1.1.1 # space
nameserver 1.1.1.2#nospace
     #leading spaces
nameserver 1.1.1.3",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1, IP_1_1_1_2, IP_1_1_1_3]);
    }

    #[test]
    fn test_parse_resolv_conf_with_invalid_content() {
        let res = parse_resolv_conf(
            "hey I am not known
nameserver 1.1.1.1
nameserver 1.1.1.2 somestuff here
abc
nameserver 1.1.1.3",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1, IP_1_1_1_2, IP_1_1_1_3]);
    }

    #[test]
    fn test_parse_resolv_conf_truncate_to_three() {
        let res = parse_resolv_conf(
            "nameserver 1.1.1.1
nameserver 1.1.1.2
nameserver 1.1.1.3
nameserver 1.1.1.4
nameserver 1.2.3.4",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_1_1_1_1, IP_1_1_1_2, IP_1_1_1_3]);
    }

    #[test]
    fn test_parse_resolv_conf_with_invalid_ip() {
        parse_resolv_conf("nameserver abc").expect_err("invalid ip must error");
    }

    #[test]
    fn test_parse_resolv_ipv6() {
        let res = parse_resolv_conf(
            "nameserver fdfd:733b:dc3:220b::2
nameserver 1.1.1.2",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_FDFD_733B_DC3_220B_2, IP_1_1_1_2]);
    }

    #[test]
    fn test_parse_resolv_ipv6_link_local_zone() {
        // Using lo here because we know that will always be id 1 and we
        // cannot assume any other interface name here.
        let res = parse_resolv_conf(
            "nameserver fe80::1%lo
",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_FE80_1]);
    }

    #[test]
    fn test_parse_resolv_ipv6_link_local_zone_id() {
        let res = parse_resolv_conf(
            "nameserver fe80::1%1
",
        )
        .expect("failed to parse");
        assert_eq!(res, vec![IP_FE80_1]);
    }
}
