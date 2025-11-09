#![allow(dead_code)]
use common::{Endpoint, PodTask, ServiceTask};
use etcd_client::EventType;
use futures::StreamExt;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::LowerName;
use hickory_proto::rr::Name;
use hickory_proto::rr::rdata::SRV;
use hickory_proto::rr::{RData, Record, RecordSet, RecordType};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_server::ServerFuture;
use hickory_server::authority::{
    Authority, AuthorityObject, Catalog, LookupControlFlow, LookupOptions, LookupRecords,
    MessageRequest, ZoneType,
};
use hickory_server::server::RequestInfo;
use hickory_server::store::forwarder::ForwardAuthority;
use log::{debug, error, info};
use std::env;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tonic::async_trait;

use crate::api::xlinestore::XlineStore;
use crate::dns::object_cache::{DnsObjectCache, EndpointRecord, PodRecord, ServiceRecord};

pub struct XlineAuthority {
    pub origin: LowerName,
    pub object_cache: Arc<DnsObjectCache>,
    pub xline_store: Arc<XlineStore>,
}

#[async_trait]
impl Authority for XlineAuthority {
    type Lookup = LookupRecords;

    fn zone_type(&self) -> ZoneType {
        ZoneType::Primary
    }

    fn origin(&self) -> &LowerName {
        &self.origin
    }

    fn is_axfr_allowed(&self) -> bool {
        false
    }

    fn can_validate_dnssec(&self) -> bool {
        false
    }

    async fn update(&self, _: &MessageRequest) -> Result<bool, ResponseCode> {
        Ok(false)
    }

    async fn search(
        &self,
        request: RequestInfo<'_>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        debug!("DNS search for: {:?}", request.query.name());
        <XlineAuthority as Authority>::lookup(
            self,
            request.query.name(),
            request.query.query_type(),
            lookup_options,
        )
        .await
    }

    async fn get_nsec_records(
        &self,
        name: &LowerName,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        LookupControlFlow::Continue(Ok(LookupRecords::Records {
            lookup_options,
            records: Arc::new(RecordSet::new(name.clone().into(), RecordType::NSEC, 0)),
        }))
    }

    async fn lookup(
        &self,
        name: &LowerName,
        rtype: RecordType,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        debug!("DNS lookup for: {name:?}");

        // 1) SRV handling
        if rtype == RecordType::SRV
            && let Some(set) = self.build_srv_recordset(name).await
        {
            return LookupControlFlow::Continue(Ok(LookupRecords::Records {
                lookup_options,
                records: Arc::new(set),
            }));
        }

        // 2) Service A / headless A handling
        if let Some(set) = self
            .build_service_or_headless_a_recordset(name, rtype)
            .await
        {
            return LookupControlFlow::Continue(Ok(LookupRecords::Records {
                lookup_options,
                records: Arc::new(set),
            }));
        }

        // 3) Pod A handling
        if let Some(set) = self.build_pod_a_recordset(name, rtype).await {
            return LookupControlFlow::Continue(Ok(LookupRecords::Records {
                lookup_options,
                records: Arc::new(set),
            }));
        }

        LookupControlFlow::Continue(Ok(LookupRecords::Empty))
    }
}

impl XlineAuthority {
    pub async fn init_from_store(&self, store: &XlineStore) -> anyhow::Result<()> {
        let pods = store.list_pods().await?;
        let mut pod_cache = self.object_cache.pod_cache.write().await;
        info!("DNS server get pods: {pods:?}");
        for pod in pods {
            let ns = pod.metadata.namespace.clone();
            let ip_str = pod.status.pod_ip.clone().unwrap_or_default();
            let ip_only = ip_str.split('/').next().unwrap();
            let ip = ip_only.parse().ok();
            let pod_ip_with_dashes = ip_only.replace('.', "-");
            info!("DNS server insert PodRecord: {pod_ip_with_dashes}, ns: {ns}");
            pod_cache.insert(
                (ns.clone(), pod_ip_with_dashes.clone()),
                PodRecord {
                    name: pod_ip_with_dashes,
                    namespace: ns,
                    pod_ip: ip,
                },
            );
        }
        info!("DNS server init_from_store pod_cache: {pod_cache:?}");
        drop(pod_cache);

        let services = store.list_services().await?;
        info!("DNS server get services: {services:?}");
        let mut svc_cache = self.object_cache.service_cache.write().await;
        for svc in services {
            let (ns, name) = (svc.metadata.namespace.clone(), svc.metadata.name.clone());
            let ip = svc
                .spec
                .cluster_ip
                .as_ref()
                .and_then(|ipstr| ipstr.parse().ok());
            info!("DNS server insert ServiceRecord : {name}");
            svc_cache.insert(
                (ns.clone(), name.clone()),
                ServiceRecord {
                    name,
                    namespace: ns,
                    cluster_ip: ip,
                    ports: svc.spec.ports.clone(),
                },
            );
        }
        drop(svc_cache);

        // endpoints
        let endpoints = store.list_endpoints().await?;
        info!("DNS server get endpoints: {endpoints:?}");
        let mut ep_cache = self.object_cache.endpoints_cache.write().await;
        for ep in endpoints {
            let ns = ep.metadata.namespace.clone();
            let name = ep.metadata.name.clone();
            info!("DNS server insert Endpoint: {name} in ns {ns}");
            ep_cache.insert(
                (ns.clone(), name.clone()),
                EndpointRecord {
                    name,
                    namespace: ns,
                    subsets: ep.subsets.clone(),
                },
            );
        }
        drop(ep_cache);

        Ok(())
    }

    pub async fn start_watch_tasks(self: Arc<Self>, start_rev: i64) {
        // pods
        let pod_cache = Arc::clone(&self.object_cache.pod_cache);
        let xline_store = Arc::clone(&self.xline_store);

        tokio::spawn(async move {
            let (mut watcher, mut stream) = xline_store.watch_pods(start_rev).await.unwrap();
            while let Some(resp) = stream.next().await {
                match resp {
                    Ok(resp) => {
                        for event in resp.events() {
                            match event.event_type() {
                                EventType::Put => {
                                    if let Some(kv) = event.kv()
                                        && let Ok(pod) =
                                            serde_yaml::from_slice::<PodTask>(kv.value())
                                    {
                                        let ns = pod.metadata.namespace.clone();
                                        let ip_str = pod.status.pod_ip.clone().unwrap_or_default();
                                        let ip_only = ip_str.split('/').next().unwrap();
                                        let ip = ip_only.parse().ok();
                                        let pod_ip_with_dashes = ip_only.replace('.', "-");
                                        info!(
                                            "DNS server insert PodRecord: {pod_ip_with_dashes}, ns: {ns}"
                                        );
                                        pod_cache.write().await.insert(
                                            (ns.clone(), pod_ip_with_dashes.clone()),
                                            PodRecord {
                                                name: pod_ip_with_dashes,
                                                namespace: ns,
                                                pod_ip: ip,
                                            },
                                        );
                                    }
                                }
                                EventType::Delete => {
                                    if let Some(kv) = event.prev_kv() {
                                        // etcd key: /registry/pods/{namespace}/{name}
                                        if let Ok(pod) =
                                            serde_yaml::from_slice::<PodTask>(kv.value())
                                        {
                                            let ns = pod.metadata.namespace.clone();
                                            let ip_str =
                                                pod.status.pod_ip.clone().unwrap_or_default();
                                            let ip_only = ip_str.split('/').next().unwrap();
                                            let pod_ip_with_dashes = ip_only.replace('.', "-");
                                            info!(
                                                "DNS server delete PodRecord : {pod_ip_with_dashes}"
                                            );
                                            pod_cache
                                                .write()
                                                .await
                                                .remove(&(ns, pod_ip_with_dashes));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "rks::dns::watch_tasks",
                            "watch error: {e}"
                        );
                        break;
                    }
                }
            }
            watcher.cancel().await.ok();
        });

        // endpoints
        let ep_cache = Arc::clone(&self.object_cache.endpoints_cache);
        let xline_store = Arc::clone(&self.xline_store);

        tokio::spawn(async move {
            let (mut watcher, mut stream) = xline_store.watch_endpoints(start_rev).await.unwrap();
            while let Some(resp) = stream.next().await {
                match resp {
                    Ok(resp) => {
                        for event in resp.events() {
                            match event.event_type() {
                                EventType::Put => {
                                    if let Some(kv) = event.kv()
                                        && let Ok(ep) =
                                            serde_yaml::from_slice::<Endpoint>(kv.value())
                                    {
                                        let ns = ep.metadata.namespace.clone();
                                        let name = ep.metadata.name.clone();
                                        info!("DNS server insert Endpoint: {name} in ns {ns}");
                                        ep_cache.write().await.insert(
                                            (ns.clone(), name.clone()),
                                            EndpointRecord {
                                                name,
                                                namespace: ns,
                                                subsets: ep.subsets.clone(),
                                            },
                                        );
                                    }
                                }
                                EventType::Delete => {
                                    if let Some(kv) = event.prev_kv() {
                                        let key_str = String::from_utf8_lossy(kv.key());
                                        let parts: Vec<&str> = key_str
                                            .trim_start_matches("/registry/endpoints/")
                                            .split('/')
                                            .collect();
                                        if parts.len() >= 2 {
                                            let ns = parts[0].to_string();
                                            let name = parts[1].to_string();
                                            ep_cache.write().await.remove(&(ns, name));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "rks::dns::watch_endpoints",
                            "watch error: {e}"
                        );
                        break;
                    }
                }
            }
            watcher.cancel().await.ok();
        });

        // services
        let svc_cache = Arc::clone(&self.object_cache.service_cache);
        let xline_store = Arc::clone(&self.xline_store);

        tokio::spawn(async move {
            let (mut watcher, mut stream) = xline_store.watch_services(start_rev).await.unwrap();
            while let Some(resp) = stream.next().await {
                match resp {
                    Ok(resp) => {
                        for event in resp.events() {
                            match event.event_type() {
                                EventType::Put => {
                                    if let Some(kv) = event.kv()
                                        && let Ok(svc) =
                                            serde_yaml::from_slice::<ServiceTask>(kv.value())
                                    {
                                        let (ns, name) = (
                                            svc.metadata.namespace.clone(),
                                            svc.metadata.name.clone(),
                                        );
                                        let ip = svc
                                            .spec
                                            .cluster_ip
                                            .as_ref()
                                            .and_then(|s| s.parse::<Ipv4Addr>().ok());
                                        svc_cache.write().await.insert(
                                            (ns.clone(), name.clone()),
                                            ServiceRecord {
                                                name,
                                                namespace: ns,
                                                cluster_ip: ip,
                                                ports: svc.spec.ports.clone(),
                                            },
                                        );
                                    }
                                }
                                EventType::Delete => {
                                    if let Some(kv) = event.prev_kv() {
                                        let key_str = String::from_utf8_lossy(kv.key());
                                        let parts: Vec<&str> = key_str
                                            .trim_start_matches("/registry/services/")
                                            .split('/')
                                            .collect();
                                        if parts.len() >= 2 {
                                            let ns = parts[0].to_string();
                                            let name = parts[1].to_string();
                                            svc_cache.write().await.remove(&(ns, name));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "rks::dns::watch_services",
                            "watch error: {e}"
                        );
                        break;
                    }
                }
            }
            watcher.cancel().await.ok();
        });
    }

    pub async fn start(
        origin: LowerName,
        xline_store: Arc<XlineStore>,
    ) -> anyhow::Result<Arc<Self>> {
        let object_cache = Arc::new(DnsObjectCache::new());
        let authority = Arc::new(Self {
            origin,
            object_cache: Arc::clone(&object_cache),
            xline_store: Arc::clone(&xline_store),
        });
        info!("DNS server init_from_store");
        authority.init_from_store(&xline_store).await?;
        info!("DNS server after init_from_store object_cache: {object_cache:?}");
        let (_, rev) = xline_store.pods_snapshot_with_rev().await?;

        let authority_clone = Arc::clone(&authority);
        tokio::spawn(async move {
            authority_clone.start_watch_tasks(rev + 1).await;
        });

        Ok(authority)
    }

    async fn build_srv_recordset(&self, name: &LowerName) -> Option<RecordSet> {
        // SRV queries have the form: _port._proto.<service>.<ns>.svc.cluster.local.
        if let Some((port_name, _proto, svc_name, ns)) = parse_srv_query(name, &self.origin) {
            // prefer service-level SRV pointing to service FQDN
            let svc_cache = self.object_cache.service_cache.read().await;
            if let Some(svc) = svc_cache.get(&(ns.clone(), svc_name.clone()))
                && svc.cluster_ip.is_some()
            {
                // find matching port by name in the service ports
                let matched_port = svc.ports.iter().find_map(|sp| {
                    sp.name.as_ref().and_then(|n| {
                        if n == &port_name {
                            Some(sp.port as u16)
                        } else {
                            None
                        }
                    })
                });

                if let Some(port_num) = matched_port {
                    let mut set = RecordSet::new(name.clone().into(), RecordType::SRV, 30);
                    let target = format!("{}.{}.svc.{}", svc_name, ns, self.origin);
                    if let Ok(target_name) = Name::from_str(&target) {
                        // Kubernetes typically uses priority 0 and weight 100 for service-level SRV records
                        let rdata = RData::SRV(SRV::new(0, 100, port_num, target_name));
                        set.insert(Record::from_rdata(name.clone().into(), 30, rdata), 0);
                    }
                    if !set.is_empty() {
                        return Some(set);
                    }
                }
            }

            // fallback to endpoints-based SRV (headless)
            let ep_cache = self.object_cache.endpoints_cache.read().await;
            if let Some(ep) = ep_cache.get(&(ns.clone(), svc_name.clone())) {
                let mut set = RecordSet::new(name.clone().into(), RecordType::SRV, 30);
                for subset in &ep.subsets {
                    for port in &subset.ports {
                        if port.name.as_deref() == Some(&port_name) {
                            let port_num = port.port as u16;
                            for addr in &subset.addresses {
                                if addr.ip.parse::<Ipv4Addr>().is_ok() {
                                    // target is the pod host name used by pod A records
                                    let pod_host = format!(
                                        "{}.pod.{}.{}",
                                        addr.ip.replace('.', "-"),
                                        ns,
                                        self.origin
                                    );
                                    if let Ok(target_name) = Name::from_str(&pod_host) {
                                        let rdata =
                                            RData::SRV(SRV::new(0, 0, port_num, target_name));
                                        set.insert(
                                            Record::from_rdata(name.clone().into(), 30, rdata),
                                            0,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                if !set.is_empty() {
                    return Some(set);
                }
            }
        }
        None
    }

    async fn build_service_or_headless_a_recordset(
        &self,
        name: &LowerName,
        rtype: RecordType,
    ) -> Option<RecordSet> {
        if let Some((svc_name, ns)) = parse_service_query(name, &self.origin) {
            let svc_cache = self.object_cache.service_cache.read().await;
            if let Some(svc) = svc_cache.get(&(ns.clone(), svc_name.clone())) {
                // ClusterIP service -> return cluster IP
                if rtype == RecordType::A {
                    if let Some(ip) = svc.cluster_ip {
                        let mut set = RecordSet::new(name.clone().into(), RecordType::A, 30);
                        set.insert(
                            Record::from_rdata(name.clone().into(), 30, RData::A(ip.into())),
                            0,
                        );
                        return Some(set);
                    }

                    // headless service: lookup endpoints for backends
                    let ep_cache = self.object_cache.endpoints_cache.read().await;
                    if let Some(ep) = ep_cache.get(&(ns.clone(), svc_name.clone())) {
                        let mut set = RecordSet::new(name.clone().into(), RecordType::A, 30);
                        for subset in &ep.subsets {
                            for addr in subset.addresses.iter() {
                                if let Ok(ipv4) = addr.ip.parse::<Ipv4Addr>() {
                                    set.insert(
                                        Record::from_rdata(
                                            name.clone().into(),
                                            30,
                                            RData::A(ipv4.into()),
                                        ),
                                        0,
                                    );
                                }
                            }
                        }

                        if !set.is_empty() {
                            return Some(set);
                        }
                    }
                }
            }
        }
        None
    }

    async fn build_pod_a_recordset(
        &self,
        name: &LowerName,
        rtype: RecordType,
    ) -> Option<RecordSet> {
        if let Some((pod_name, ns)) = parse_pod_query(name, &self.origin) {
            info!("DNS lookup the pod_name: {pod_name}, ns: {ns}");
            let cache = self.object_cache.pod_cache.read().await;
            if let Some(pod) = cache.get(&(ns.clone(), pod_name.clone()))
                && rtype == RecordType::A
                && let Some(ip) = pod.pod_ip
            {
                info!("DNS find the Record: {pod:?}");
                let mut set = RecordSet::new(name.clone().into(), RecordType::A, 30);
                set.insert(
                    Record::from_rdata(name.clone().into(), 30, RData::A(ip.into())),
                    0,
                );
                return Some(set);
            }
            info!("DNS not find the Record");
        }
        None
    }
}

pub async fn run_dns_server(xline_store: Arc<XlineStore>, port: u16) -> anyhow::Result<()> {
    let origin = LowerName::from_str("cluster.local.")?;
    let xline_authority = XlineAuthority::start(origin.clone(), xline_store).await?;

    let mut catalog = Catalog::new();

    let xline_authority: Arc<dyn AuthorityObject> = xline_authority;
    catalog.upsert(origin, vec![xline_authority]);

    let forwarder = ForwardAuthority::builder(TokioConnectionProvider::default())
        .map_err(|e| anyhow::anyhow!(e))?
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    catalog.upsert(LowerName::from(Name::root()), vec![Arc::new(forwarder)]);

    let mut server = ServerFuture::new(catalog);
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let udp_socket = UdpSocket::bind(addr).await?;
    server.register_socket(udp_socket);

    info!("DNS server listening on {addr}");

    server.block_until_done().await?;
    Ok(())
}

fn parse_service_query(name: &LowerName, origin: &LowerName) -> Option<(String, String)> {
    let labels: Vec<_> = name
        .iter()
        .map(|l| std::str::from_utf8(l).unwrap_or_default())
        .collect();

    if !labels.contains(&"svc") {
        return None;
    }
    // "nginx.default.svc.cluster.local." -> ("nginx", "default")
    let num = origin.num_labels();

    let mut iter = name.iter().take((name.num_labels() - num).into());

    let svc = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    let ns = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    Some((svc, ns))
}

fn parse_pod_query(name: &LowerName, origin: &LowerName) -> Option<(String, String)> {
    let labels: Vec<_> = name
        .iter()
        .map(|l| std::str::from_utf8(l).unwrap_or_default())
        .collect();

    if !labels.contains(&"pod") {
        return None;
    }

    let num = origin.num_labels();
    let mut iter = name.iter().take((name.num_labels() - num).into());

    let pod = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    let ns = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    Some((pod, ns))
}

fn parse_srv_query(
    name: &LowerName,
    origin: &LowerName,
) -> Option<(String, String, String, String)> {
    // expected labels: _port._proto.<service>.<ns>.svc... (relative to origin)
    let labels: Vec<_> = name
        .iter()
        .map(|l| std::str::from_utf8(l).unwrap_or_default())
        .collect();

    if labels.len() < 4 {
        return None;
    }

    let num = origin.num_labels();
    let mut iter = name.iter().take((name.num_labels() - num).into());

    let port_label = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default();

    let proto_label = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default();

    let svc = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    let ns = iter
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .unwrap_or_default()
        .to_string();

    let port_name = port_label
        .strip_prefix('_')
        .unwrap_or(port_label)
        .to_string();
    let proto = proto_label
        .strip_prefix('_')
        .unwrap_or(proto_label)
        .to_string();

    Some((port_name, proto, svc, ns))
}

pub async fn setup_iptable(dns_ip: String, dns_port: u16) -> anyhow::Result<()> {
    let br_name = env::var("BR_NAME").unwrap_or_else(|_| "cni0".to_string());
    let ipt = iptables::new(false).map_err(|e| anyhow::anyhow!("failed to init iptables: {e}"))?;

    let rule1 = format!("-i {br_name} -p udp --dport 53 -j REDIRECT --to-ports {dns_port}");
    let rule2 = format!("-i {br_name} -p tcp --dport 53 -j REDIRECT --to-ports {dns_port}");
    let rule3 =
        format!("-p udp -d {dns_ip} --dport 53 -j DNAT --to-destination {dns_ip}:{dns_port}");
    let rule4 =
        format!("-p tcp -d {dns_ip} --dport 53 -j DNAT --to-destination {dns_ip}:{dns_port}");

    let rules = [
        ("rule1", &rule1),
        ("rule2", &rule2),
        ("rule3", &rule3),
        ("rule4", &rule4),
    ];

    for (name, rule) in rules {
        if !ipt
            .exists("nat", "PREROUTING", rule)
            .map_err(|e| anyhow::anyhow!("failed to check {name}: {e}"))?
        {
            ipt.append("nat", "PREROUTING", rule)
                .map_err(|e| anyhow::anyhow!("failed to insert {name}: {e}"))?;
            info!("Added iptables rule: {rule}");
        } else {
            info!("Rule already exists: {rule}");
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn cleanup_iptable(dns_ip: String, dns_port: u16) -> anyhow::Result<()> {
    let br_name = env::var("BR_NAME").unwrap_or_else(|_| "cni0".to_string());
    let ipt = iptables::new(false).map_err(|e| anyhow::anyhow!("failed to init iptables: {e}"))?;

    let rule1 = format!("-i {br_name} -p udp --dport 53 -j REDIRECT --to-ports {dns_port}");
    let rule2 = format!("-i {br_name} -p tcp --dport 53 -j REDIRECT --to-ports {dns_port}");
    let rule3 =
        format!("-p udp -d {dns_ip} --dport 53 -j DNAT --to-destination {dns_ip}:{dns_port}");
    let rule4 =
        format!("-p tcp -d {dns_ip} --dport 53 -j DNAT --to-destination {dns_ip}:{dns_port}");

    let rules = [
        ("rule1", &rule1),
        ("rule2", &rule2),
        ("rule3", &rule3),
        ("rule4", &rule4),
    ];
    for (name, rule) in rules {
        if ipt
            .exists("nat", "PREROUTING", rule)
            .map_err(|e| anyhow::anyhow!("failed to check {name}: {e}"))?
        {
            ipt.delete("nat", "PREROUTING", rule)
                .map_err(|e| anyhow::anyhow!("failed to delete {name}: {e}"))?;
            info!("Deleted iptables rule: {rule}");
        } else {
            info!("Rule does not exist, skipping delete: {rule}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::LowerName;
    use std::str::FromStr;

    #[test]
    fn test_parse_service_query() {
        let name = LowerName::from_str("nginx.default.svc.cluster.local.").unwrap();
        let origin = LowerName::from_str("svc.cluster.local.").unwrap();

        let (svc, ns) = parse_service_query(&name, &origin).unwrap();

        assert_eq!(svc, "nginx");
        assert_eq!(ns, "default");
    }
}
