#![allow(dead_code)]

use std::str;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use common::lease::{Event, EventType, Lease, LeaseAttrs};
use ipnetwork::{Ipv4Network, Ipv6Network};

use log::{error, info};

use tokio::sync::mpsc::{self, Sender};

use crate::network::lease::{LeaseWatchResult, LeaseWatcher};
use libnetwork::config::NetworkConfig;

#[async_trait]
pub trait Manager: Send + Sync {
    async fn get_network_config(&self) -> Result<NetworkConfig>;

    async fn handle_subnet_file(
        &self,
        path: &str,
        config: &NetworkConfig,
        ip_masq: bool,
        sn: Ipv4Network,
        sn6: Ipv6Network,
        mtu: i32,
    ) -> Result<()>;

    async fn acquire_lease(&self, attrs: &LeaseAttrs) -> Result<Lease>;

    async fn renew_lease(&self, lease: &Lease) -> Result<()>;

    async fn watch_lease(
        &self,
        sn: Ipv4Network,
        sn6: Ipv6Network,
        sender: mpsc::Sender<Vec<LeaseWatchResult>>,
    ) -> Result<()>;

    async fn watch_leases(&self, sender: mpsc::Sender<Vec<LeaseWatchResult>>) -> Result<()>;

    async fn complete_lease(&self, lease: &Lease) -> Result<()>;

    async fn get_stored_mac_addresses(&self) -> (String, String);

    async fn get_stored_public_ip(&self) -> (String, String);

    fn name(&self) -> String;
}

pub async fn watch_leases(
    sm: Arc<dyn Manager>,
    own_lease: Lease,
    receiver: Sender<Vec<Event>>,
) -> Result<()> {
    let mut lw = LeaseWatcher {
        own_lease,
        leases: vec![],
    };
    let (tx_watch, mut rx_watch) = mpsc::channel(100);

    // Spawn watcher task
    let sm_clone = sm.clone();
    tokio::spawn(async move {
        if let Err(e) = sm_clone.watch_leases(tx_watch).await {
            error!("could not watch leases: {e}");
        }
    });

    while let Some(watch_results) = rx_watch.recv().await {
        for wr in watch_results {
            let batch = if !wr.events.is_empty() {
                lw.update(wr.events)
            } else {
                lw.reset(wr.snapshot)
            };

            for (i, evt) in batch.iter().enumerate() {
                info!("Batch elem [{i}] is {evt:?}");
            }

            if !batch.is_empty() {
                let _ = receiver.send(batch).await;
            }
        }
    }
    Ok(())
}

pub async fn watch_lease(
    sm: Arc<dyn Manager>,
    sn: Ipv4Network,
    sn6: Ipv6Network,
    receiver: Sender<Event>,
) -> Result<()> {
    let (tx_watch, mut rx_watch) = mpsc::channel::<Vec<LeaseWatchResult>>(100);

    let sm_clone = sm.clone();

    // Spawn watcher task
    tokio::spawn(async move {
        match sm_clone.watch_lease(sn, sn6, tx_watch).await {
            Err(e) if e.to_string().contains("cancelled") => {
                info!("Context cancelled, closing receiver channel");
            }
            Err(e) => {
                error!("Subnet watch failed: {e}");
            }
            Ok(_) => {}
        }
    });

    while let Some(watch_results) = rx_watch.recv().await {
        for wr in watch_results {
            if let Some(lease) = wr.snapshot.first() {
                let event_added = Event {
                    event_type: EventType::Added,
                    lease: Some(lease.clone()),
                };
                let _ = receiver.send(event_added).await;
            } else if let Some(event) = wr.events.first() {
                let _ = receiver.send(event.clone()).await;
            } else {
                info!("WatchLease: empty event received");
            }
        }
    }

    info!("leaseWatchChan channel closed");
    Ok(())
}
