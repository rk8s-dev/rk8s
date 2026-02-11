use crate::backend::DNSBackend;
use crate::error::AardvarkResult;
use arc_swap::ArcSwap;
use arc_swap::Guard;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use hickory_client::{
    client::Client, proto::rr::rdata, proto::rr::Name, proto::xfer::SerialMessage,
};
use hickory_proto::{
    op::{Message, MessageType, ResponseCode},
    rr::{RData, Record, RecordType},
    runtime::{iocompat::AsyncIoTokioAsStd, TokioRuntimeProvider},
    tcp::{TcpClientStream, TcpStream},
    udp::{UdpClientStream, UdpStream},
    xfer::{dns_handle::DnsHandle, BufDnsStreamHandle, DnsRequest},
    DnsStreamHandle,
};
use log::{debug, error, trace, warn};
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::UdpSocket;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

pub const DNS_PORT: u16 = 53;

pub struct CoreDns {
    rx: flume::Receiver<()>, // kill switch receiver
    inner: CoreDnsData,
}

#[derive(Clone)]
struct CoreDnsData {
    network_name: String,                     // raw network name
    backend: &'static ArcSwap<DNSBackend>,    // server's data store
    no_proxy: bool,                           // do not forward to external resolvers
    nameservers: Arc<Mutex<Vec<SocketAddr>>>, // host nameservers from resolv.conf
}

enum Protocol {
    Udp,
    Tcp,
}

impl CoreDns {
    // Most of the arg can be removed in design refactor.
    // so dont create a struct for this now.
    pub fn new(
        network_name: String,
        backend: &'static ArcSwap<DNSBackend>,
        rx: flume::Receiver<()>,
        no_proxy: bool,
        nameservers: Arc<Mutex<Vec<SocketAddr>>>,
    ) -> Self {
        CoreDns {
            rx,
            inner: CoreDnsData {
                network_name,
                backend,
                no_proxy,
                nameservers,
            },
        }
    }

    pub async fn run(
        &self,
        udp_socket: UdpSocket,
        tcp_listener: TcpListener,
    ) -> AardvarkResult<()> {
        let address = udp_socket.local_addr()?;
        let (mut receiver, sender_original) =
            UdpStream::<TokioRuntimeProvider>::with_bound(udp_socket, address);

        loop {
            tokio::select! {
                _ = self.rx.recv_async() => {
                    break;
                },
                v = receiver.next() => {
                    let msg_received = match v {
                        Some(value) => value,
                        None => {
                            // None received, nothing to process so continue
                            debug!("None recevied from stream, continue the loop");
                            continue;
                        }
                    };
                    Self::process_message(&self.inner, msg_received, &sender_original, Protocol::Udp).await;
                },
                res = tcp_listener.accept() => {
                    match res {
                        Ok((sock,addr)) => {
                            tokio::spawn(Self::process_tcp_stream(self.inner.clone(), sock, addr));
                        }
                        Err(e) => {
                            error!("Failed to accept new tcp connection: {e}");
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn process_tcp_stream(
        data: CoreDnsData,
        stream: tokio::net::TcpStream,
        peer: SocketAddr,
    ) {
        let (mut hickory_stream, sender_original) =
            TcpStream::from_stream(AsyncIoTokioAsStd(stream), peer);

        loop {
            // It is possible for a client to keep the tcp socket open forever and never send any data,
            // we do not want this so add a 3s timeout then we close the socket.
            match tokio::time::timeout(Duration::from_secs(3), hickory_stream.next()).await {
                Ok(message) => match message {
                    Some(msg) => {
                        Self::process_message(&data, msg, &sender_original, Protocol::Tcp).await
                    }
                    // end of stream
                    None => break,
                },
                Err(_) => {
                    debug!(
                        "Tcp connection {peer} was cancelled after 3s as it took too long to receive message"
                    );
                    break;
                }
            }
        }
    }

    async fn process_message(
        data: &CoreDnsData,
        msg_received: Result<SerialMessage, Error>,
        sender_original: &BufDnsStreamHandle,
        proto: Protocol,
    ) {
        let msg = match msg_received {
            Ok(msg) => msg,
            Err(e) => {
                error!("Error parsing dns message {e:?}");
                return;
            }
        };
        let backend = data.backend.load();
        let src_address = msg.addr();
        let mut sender = sender_original.with_remote_addr(src_address);
        let (request_name, record_type, mut req) = match parse_dns_msg(msg) {
            Some((name, record_type, req)) => (name, record_type, req),
            _ => {
                error!("None received while parsing dns message, this is not expected server will ignore this message");
                return;
            }
        };
        let request_name_string = request_name.to_string();

        // Create debug and trace info for key parameters.
        trace!("server network name: {:?}", data.network_name);
        debug!("request source address: {src_address:?}");
        trace!("requested record type: {record_type:?}");
        debug!(
            "checking if backend has entry for: {:?}",
            &request_name_string
        );
        trace!("server backend.name_mappings: {:?}", backend.name_mappings);
        trace!("server backend.ip_mappings: {:?}", backend.ip_mappings);

        match record_type {
            RecordType::PTR => {
                if let Some(msg) = reply_ptr(&request_name_string, &backend, src_address, &req) {
                    reply(&mut sender, src_address, &msg);
                    return;
                }
                // No match found, forwarding below.
            }
            RecordType::A | RecordType::AAAA => {
                if let Some(msg) = reply_ip(
                    &request_name_string,
                    &request_name,
                    &data.network_name,
                    record_type,
                    &backend,
                    src_address,
                    &mut req,
                ) {
                    reply(&mut sender, src_address, msg);
                    return;
                }
                // No match found, forwarding below.
            }

            // TODO: handle MX here like docker does

            // We do not handle this request type so do nothing,
            // we forward the request to upstream resolvers below.
            _ => {}
        };

        // are we allowed to forward?
        if data.no_proxy
            || backend.ctr_is_internal(&src_address.ip())
            || request_name_string.ends_with(&backend.search_domain)
        {
            let mut nx_message = req.clone();
            nx_message.set_response_code(ResponseCode::NXDomain);
            reply(&mut sender, src_address, &nx_message);
        } else {
            debug!(
                "Forwarding dns request for {} type: {}",
                &request_name_string, record_type
            );
            let mut nameservers = Vec::new();
            // Add resolvers configured for container
            if let Some(Some(dns_servers)) = backend.ctr_dns_server.get(&src_address.ip()) {
                for dns_server in dns_servers.iter() {
                    nameservers.push(SocketAddr::new(*dns_server, DNS_PORT));
                }
                // Add network scoped resolvers only if container specific resolvers were not configured
            } else if let Some(network_dns_servers) =
                backend.get_network_scoped_resolvers(&src_address.ip())
            {
                for dns_server in network_dns_servers.iter() {
                    nameservers.push(SocketAddr::new(*dns_server, DNS_PORT));
                }
            }
            // Use host resolvers if no custom resolvers are set for the container.
            if nameservers.is_empty() {
                nameservers.clone_from(&data.nameservers.lock().expect("lock nameservers"));
            }

            match proto {
                Protocol::Udp => {
                    tokio::spawn(Self::forward_to_servers(
                        nameservers,
                        sender,
                        src_address,
                        req,
                        proto,
                    ));
                }
                Protocol::Tcp => {
                    // we already spawned a new future when we read the message so there is no need to spawn another one
                    Self::forward_to_servers(nameservers, sender, src_address, req, proto).await;
                }
            }
        }
    }

    async fn forward_to_servers(
        nameservers: Vec<SocketAddr>,
        mut sender: BufDnsStreamHandle,
        src_address: SocketAddr,
        req: Message,
        proto: Protocol,
    ) {
        let mut timeout = DEFAULT_TIMEOUT;
        // Remember do not divide by 0.
        if !nameservers.is_empty() {
            timeout = Duration::from_secs(5) / nameservers.len() as u32
        }
        // forward dns request to hosts's /etc/resolv.conf
        for addr in nameservers {
            let (client, handle) = match proto {
                Protocol::Udp => {
                    let stream = UdpClientStream::builder(addr, TokioRuntimeProvider::default())
                        .with_timeout(Some(timeout))
                        .build();
                    let (cl, bg) = match Client::connect(stream).await {
                        Ok(a) => a,
                        Err(e) => {
                            debug!("Failed to connect to {addr}: {e}");
                            continue;
                        }
                    };
                    let handle = tokio::spawn(bg);
                    (cl, handle)
                }
                Protocol::Tcp => {
                    let (stream, sender) = TcpClientStream::new(
                        addr,
                        None,
                        Some(timeout),
                        TokioRuntimeProvider::default(),
                    );
                    //let (stream, sender) = TcpClientStream::<
                    //    AsyncIoTokioAsStd<tokio::net::TcpStream>,
                    //>::with_timeout(addr, timeout);
                    let (cl, bg) = match Client::with_timeout(stream, sender, timeout, None).await {
                        Ok(a) => a,
                        Err(e) => {
                            debug!("Failed to connect to {addr}: {e}");
                            continue;
                        }
                    };
                    let handle = tokio::spawn(bg);
                    (cl, handle)
                }
            };

            if let Some(resp) = forward_dns_req(client, req.clone()).await {
                if reply(&mut sender, src_address, &resp).is_some() {
                    // request resolved from following resolver so
                    // break and don't try other resolvers
                    break;
                }
            }
            handle.abort();
        }
    }
}

fn reply(sender: &mut BufDnsStreamHandle, socket_addr: SocketAddr, msg: &Message) -> Option<()> {
    let id = msg.id();
    let mut msg_mut = msg.clone();
    msg_mut.set_message_type(MessageType::Response);
    // If `RD` is set and `RA` is false set `RA`.
    if msg.recursion_desired() && !msg.recursion_available() {
        msg_mut.set_recursion_available(true);
    }
    let response = SerialMessage::new(msg_mut.to_vec().ok()?, socket_addr);

    match sender.send(response) {
        Ok(_) => {
            debug!("[{id}] success reponse");
        }
        Err(e) => {
            error!("[{id}] fail response: {e:?}");
        }
    }

    Some(())
}

fn parse_dns_msg(body: SerialMessage) -> Option<(Name, RecordType, Message)> {
    match Message::from_vec(body.bytes()) {
        Ok(msg) => {
            let mut name = Name::default();
            let mut record_type: RecordType = RecordType::A;

            let parsed_msg = format!(
                "[{}] parsed message body: {} edns: {}",
                msg.id(),
                msg.queries()
                    .first()
                    .map(|q| {
                        name = q.name().clone();
                        record_type = q.query_type();

                        format!("{} {} {}", q.name(), q.query_type(), q.query_class(),)
                    })
                    .unwrap_or_else(Default::default,),
                msg.extensions().is_some(),
            );

            debug!("parsed message {parsed_msg:?}");

            Some((name, record_type, msg))
        }
        Err(e) => {
            warn!("Failed while parsing message: {e}");
            None
        }
    }
}

async fn forward_dns_req(cl: Client, message: Message) -> Option<Message> {
    let req = DnsRequest::new(message, Default::default());
    let id = req.id();

    match cl.send(req).try_next().await {
        Ok(Some(response)) => {
            for answer in response.answers() {
                debug!(
                    "{} {} {} {} => {:#?}",
                    id,
                    answer.name(),
                    answer.record_type(),
                    answer.dns_class(),
                    answer.data(),
                );
            }
            let mut response_message = response.into_message();
            response_message.set_id(id);
            Some(response_message)
        }
        Ok(None) => {
            error!("{id} dns request got empty response");
            None
        }
        Err(e) => {
            error!("{id} dns request failed: {e}");
            None
        }
    }
}

fn reply_ptr(
    name: &str,
    backend: &Guard<Arc<DNSBackend>>,
    src_address: SocketAddr,
    req: &Message,
) -> Option<Message> {
    let ptr_lookup_ip: String;
    // Are we IPv4 or IPv6?

    match name.strip_suffix(".in-addr.arpa.") {
        Some(n) => ptr_lookup_ip = n.split('.').rev().collect::<Vec<&str>>().join("."),
        None => {
            // not ipv4
            match name.strip_suffix(".ip6.arpa.") {
                Some(n) => {
                    // ipv6 string is 39 chars max
                    let mut tmp_ip = String::with_capacity(40);
                    for (i, c) in n.split('.').rev().enumerate() {
                        tmp_ip.push_str(c);
                        // insert colon after 4 hex chars but not at the end
                        if i % 4 == 3 && i < 31 {
                            tmp_ip.push(':');
                        }
                    }
                    ptr_lookup_ip = tmp_ip;
                }
                // neither ipv4 or ipv6, something we do not understand
                None => return None,
            }
        }
    }

    trace!("Performing reverse lookup for ip: {}", &ptr_lookup_ip);

    // We should probably log malformed queries, but for now if-let should be fine.
    if let Ok(lookup_ip) = ptr_lookup_ip.parse() {
        if let Some(reverse_lookup) = backend.reverse_lookup(&src_address.ip(), &lookup_ip) {
            let mut req_clone = req.clone();
            for entry in reverse_lookup {
                if let Ok(answer) = Name::from_ascii(format!("{entry}.")) {
                    let record = Record::<RData>::from_rdata(
                        Name::from_str_relaxed(name).unwrap_or_default(),
                        0,
                        RData::PTR(rdata::PTR(answer)),
                    );
                    req_clone.add_answer(record);
                }
            }
            return Some(req_clone);
        }
    };
    None
}

fn reply_ip<'a>(
    name: &str,
    request_name: &Name,
    network_name: &str,
    record_type: RecordType,
    backend: &Guard<Arc<DNSBackend>>,
    src_address: SocketAddr,
    req: &'a mut Message,
) -> Option<&'a Message> {
    // attempt intra network resolution
    let resolved_ip_list = backend.lookup(&src_address.ip(), network_name, name)?;

    if record_type == RecordType::A {
        for record_addr in resolved_ip_list {
            if let IpAddr::V4(ipv4) = record_addr {
                // Set TTL to 0 which means client should not cache it.
                // Containers can be be restarted with a different ip at any time so allowing
                // caches here doesn't make much sense given the server is local and queries
                // should be fast enough anyway.
                let record =
                    Record::<RData>::from_rdata(request_name.clone(), 0, RData::A(rdata::A(ipv4)));
                req.add_answer(record);
            }
        }
    } else if record_type == RecordType::AAAA {
        for record_addr in resolved_ip_list {
            if let IpAddr::V6(ipv6) = record_addr {
                let record = Record::<RData>::from_rdata(
                    request_name.clone(),
                    0,
                    RData::AAAA(rdata::AAAA(ipv6)),
                );
                req.add_answer(record);
            }
        }
    }
    Some(req)
}
