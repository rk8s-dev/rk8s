#[allow(unused)]
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::fs;
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

use anyhow::{Result, anyhow};
use hickory_proto::{
    op::ResponseCode,
    rr::{LowerName, RData, RecordSet, RecordType, rdata::A},
};
use hickory_server::{
    authority::{
        Authority, LookupControlFlow, LookupOptions, LookupRecords, MessageRequest, ZoneType,
    },
    server::RequestInfo,
};
use tracing::debug;

use crate::dns::UpdateAction::Add;
use crate::dns::UpdateAction::Delete;
use crate::dns::UpdateAction::Update;
use crate::dns::{DNS_SOCKET_PATH, DNSUpdateMessage};

pub struct LocalAuthority {
    pub origin: LowerName,
    pub store: Arc<Mutex<dyn RecordStore + Send + Sync>>,
}

#[async_trait::async_trait]
impl Authority for LocalAuthority {
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

    async fn update(&self, _: &MessageRequest) -> Result<bool, ResponseCode> {
        Ok(false)
    }

    async fn search(
        &self,
        request: RequestInfo<'_>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        debug!("DNS search for: {:?}", request.query.name());
        <LocalAuthority as Authority>::lookup(
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
        _rtype: RecordType,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        // Only support A type
        let store = self.store.lock().await;
        debug!("{:?}", store);
        if let Ok(data) = store.get(name).await {
            let a = data.into_a().unwrap().0;
            let mut set = RecordSet::new(name.into(), RecordType::A, 30);
            set.add_rdata(RData::A(A(a)));

            debug!("lookup for {name}, get addr {a}");

            return LookupControlFlow::Continue(Ok(LookupRecords::Records {
                lookup_options,
                records: Arc::new(set),
            }));
        }
        return LookupControlFlow::Skip;
    }
}

impl LocalAuthority {
    pub fn _from_mem(origin: &str) -> Self {
        let map: HashMap<LowerName, RData> = HashMap::new();
        let mem_store = MemStore(map);
        Self {
            origin: LowerName::from_str(origin).unwrap(),
            store: Arc::new(Mutex::new(mem_store)),
        }
    }
    async fn handle_connection(
        self: Arc<Self>,
        mut stream: tokio::net::UnixStream,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::new(&mut stream);
        let mut buf = String::new();
        reader.read_line(&mut buf).await?;
        let msg: DNSUpdateMessage = serde_json::from_str(&buf)?;

        {
            let mut store = self.store.lock().await;
            match msg.action {
                Add => store.add(&msg.name, RData::A(A(msg.ip))).await?,
                Update => store.add(&msg.name, RData::A(A(msg.ip))).await?,
                Delete => store.del(&msg.name).await?,
            };
        }

        debug!("Current: Store: {:#?}", self.store);

        stream.write_all(b"ok\n").await?;

        Ok(())
    }

    pub async fn start_watch(self: Arc<Self>) -> Result<()> {
        if fs::metadata(DNS_SOCKET_PATH).await.is_ok() {
            fs::remove_file(DNS_SOCKET_PATH).await?;
        }
        let listener = UnixListener::bind(DNS_SOCKET_PATH)?;
        println!("RKL DNS daemon listening on {}", DNS_SOCKET_PATH);
        loop {
            let (stream, _) = listener
                .accept()
                .await
                .expect("Unix Listener failed to accept the stream");
            let this = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = this.handle_connection(stream).await {
                    eprintln!("Error: {e}");
                }
            });
        }
    }

    pub async fn start(
        origin: &str,
        store: Arc<Mutex<dyn RecordStore + Send + Sync>>,
    ) -> Result<Arc<Self>> {
        let authority = Arc::new(Self {
            origin: LowerName::from_str(origin).unwrap(),
            store,
        });
        let background = authority.clone();
        tokio::spawn(async move {
            if let Err(e) = background.start_watch().await {
                eprintln!("start_watch failed: {e}");
            }
        });
        Ok(authority)
    }
}
use std::fmt::Debug;

#[async_trait::async_trait]
pub trait RecordStore: Debug {
    async fn add(&mut self, name: &LowerName, record: RData) -> Result<()>;
    async fn del(&mut self, name: &LowerName) -> Result<()>;
    async fn get(&self, name: &LowerName) -> Result<RData>;
}

#[derive(Debug)]
pub struct MemStore(HashMap<LowerName, RData>);

#[async_trait::async_trait]
impl RecordStore for MemStore {
    async fn add(&mut self, name: &LowerName, record: RData) -> Result<()> {
        self.0.insert(name.clone(), record.clone());
        Ok(())
    }
    async fn del(&mut self, name: &LowerName) -> Result<()> {
        self.0.remove(name);
        Ok(())
    }
    async fn get(&self, name: &LowerName) -> Result<RData> {
        self.0
            .get(name)
            .ok_or_else(|| anyhow!("record not found"))
            .cloned()
    }
}

impl MemStore {
    pub fn new() -> Self {
        let map: HashMap<LowerName, RData> = HashMap::new();
        MemStore(map)
    }
}
impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}
