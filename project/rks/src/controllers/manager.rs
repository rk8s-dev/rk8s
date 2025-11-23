use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use async_trait::async_trait;
use common::ResourceKind;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::sleep;

pub static CONTROLLER_MANAGER: Lazy<Arc<ControllerManager>> =
    Lazy::new(|| Arc::new(ControllerManager::new()));

/// A watch event.
/// Contains the resource yaml.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Add { yaml: String },
    Update { old_yaml: String, new_yaml: String },
    Delete { yaml: String },
}

/// A watch response.
/// Contains the resource kind, key, and event.
#[derive(Debug, Clone)]
pub struct ResourceWatchResponse {
    pub kind: ResourceKind,
    pub key: String,
    pub event: WatchEvent,
}

/// Controller trait defines the contract for controllers managed by ControllerManager.
///
/// Controllers are used to watch rk8s resource changes (such as Pods, ReplicaSets, etc.)
/// and execute corresponding processing logic. Each controller must implement this trait and
/// register with the manager via `ControllerManager::register`.
///
/// # Usage
///
/// 1. **Implement the Controller trait**: Implement all required methods for your controller struct
/// 2. **Register the controller**: Use `ControllerManager::register` to register the controller with the manager
/// 3. **Start watching**: Call `ControllerManager::start_watch` to begin watching resource changes
///
/// # Example
///
/// ```no_run
/// use crate::controllers::manager::{Controller, ResourceWatchResponse, WatchEvent};
/// use common::ResourceKind;
/// use async_trait::async_trait;
/// use anyhow::Result;
///
/// struct MyController {
///     // Your controller state
/// }
///
/// #[async_trait]
/// impl Controller for MyController {
///     fn name(&self) -> &'static str {
///         "my-controller"
///     }
///
///     async fn init(&mut self) -> Result<()> {
///         // Initialization logic, e.g., load config, establish connections, etc.
///         Ok(())
///     }
///
///     fn watch_resources(&self) -> Vec<ResourceKind> {
///         // Return the resource types to watch
///         vec![ResourceKind::Pod, ResourceKind::ReplicaSet]
///     }
///
///     async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
///         // Handle resource change events
///         match &response.event {
///             WatchEvent::Add { yaml } => {
///                 // Handle resource add event
///             }
///             WatchEvent::Update { old_yaml, new_yaml } => {
///                 // Handle resource update event
///             }
///             WatchEvent::Delete { yaml } => {
///                 // Handle resource delete event
///             }
///         }
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait Controller: Send + Sync + 'static {
    /// Returns the controller's name, used for identification and logging.
    ///
    /// Each controller's name should be unique. It's recommended to use meaningful names,
    /// such as "replicaset-controller".
    fn name(&self) -> &'static str;

    /// Initializes the controller, called once during registration.
    ///
    /// You can perform initialization logic here, such as:
    /// - Loading configuration
    /// - Initializing internal state
    /// - Starting background tasks
    ///
    /// If initialization fails, the controller will not be registered.
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns `Ok(())`. If no initialization logic is needed,
    /// you don't need to override this method.
    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Returns the list of resource types that the controller needs to watch.
    ///
    /// The controller will only receive events for resource types declared in the list. For example:
    /// - If you return `vec![ResourceKind::Pod]`, it will only receive Pod resource events
    /// - If you return an empty list, it will not receive any events
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an empty list. You need to override this method
    /// to specify which resources to watch.
    ///
    /// # Example
    ///
    /// ```no_run
    /// fn watch_resources(&self) -> Vec<ResourceKind> {
    ///     vec![ResourceKind::Pod, ResourceKind::ReplicaSet]
    /// }
    /// ```
    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![]
    }

    /// Handles resource watch response events.
    ///
    /// This method is called when watched resources change (add, update, delete).
    /// The method executes in a separate async task, supporting concurrent processing of multiple events.
    ///
    /// # Parameters
    ///
    /// * `response` - Contains the resource kind, resource key, and event details
    ///
    /// # Error Handling
    ///
    /// If processing fails, it will automatically retry (up to 5 times with exponential backoff).
    /// If all retries fail, an error log will be recorded, but it won't affect processing of other events.
    ///
    /// # Concurrency Control
    ///
    /// The number of concurrently processed events is controlled by the `workers` parameter
    /// in `ControllerManager::register`.
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns `Ok(())`. You need to override this method
    /// to implement specific business logic.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use crate::controllers::manager::{ResourceWatchResponse, WatchEvent};
    ///
    /// async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
    ///     log::info!("Received {} resource event: {}", response.kind, response.key);
    ///     
    ///     match &response.event {
    ///         WatchEvent::Add { yaml } => {
    ///             // Parse yaml and handle add logic
    ///         }
    ///         WatchEvent::Update { old_yaml, new_yaml } => {
    ///             // Compare old and new yaml and handle update logic
    ///         }
    ///         WatchEvent::Delete { yaml } => {
    ///             // Handle delete logic
    ///         }
    ///     }
    ///     
    ///     Ok(())
    /// }
    /// ```
    #[allow(unused)]
    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        Ok(())
    }
}

/// ControllerManager manages the lifecycle and event distribution of multiple controllers.
///
/// ControllerManager is responsible for:
/// - Registering and managing multiple controllers
/// - Watching Kubernetes resource changes (Pods, ReplicaSets, etc.)
/// - Distributing resource events to corresponding controllers
/// - Controlling the number of concurrent processing tasks
/// - Providing graceful shutdown mechanism
///
/// # Workflow
///
/// 1. **Create manager**: Use the global singleton `CONTROLLER_MANAGER`
/// 2. **Register controllers**: Call `register` to register each controller
/// 3. **Start watching**: Call `start_watch` to begin watching resource changes
/// 4. **Event processing**: The manager automatically distributes events to corresponding controller queues, controllers process asynchronously
/// 5. **Shutdown**: Call `shutdown` to gracefully shut down all controllers
///
/// # Usage
///
/// ```no_run
/// use crate::controllers::manager::{ControllerManager, CONTROLLER_MANAGER};
/// use std::sync::Arc;
///
/// // Use global singleton
/// let manager = CONTROLLER_MANAGER.clone();
///
/// // Register controller (assuming MyController implements Controller trait)
/// let controller = Arc::new(RwLock::new(MyController::new()));
/// manager.clone().register(controller, 10).await?; // 10 concurrent worker threads
///
/// // Start watching (requires XlineStore instance)
/// manager.clone().start_watch(store).await?;
/// ```
///
/// # Features
///
/// - **Auto-reconnect**: Automatically reconnects when watch connection is lost, with exponential backoff
/// - **Concurrency control**: Each controller can configure maximum concurrent processing count
/// - **Auto-retry**: Automatically retries on processing failure (up to 5 times)
/// - **Graceful shutdown**: Supports stopping all controllers via `shutdown` method
pub struct ControllerManager {
    controllers: RwLock<HashMap<String, Arc<RwLock<dyn Controller>>>>,
    // a work queue per controller.
    queues: RwLock<HashMap<String, mpsc::Sender<ResourceWatchResponse>>>,
    // use for avoiding duplicates and avoid the same key gets into queue twice.
    inflight: RwLock<HashMap<String, HashSet<String>>>,
    // use for stopping the manager.
    stop_tx: watch::Sender<bool>,
}

impl ControllerManager {
    /// Creates a new ControllerManager instance.
    ///
    /// It's generally recommended to use the global singleton `CONTROLLER_MANAGER`,
    /// unless you need multiple independent manager instances.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let manager = Arc::new(ControllerManager::new());
    /// ```
    pub fn new() -> Self {
        let (stop_tx, _) = watch::channel(false);
        Self {
            controllers: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            inflight: RwLock::new(HashMap::new()),
            stop_tx,
        }
    }

    /// Registers a controller and starts its event processing loop.
    ///
    /// This method will:
    /// 1. Call the controller's `init` method for initialization
    /// 2. Create a work queue for the controller (capacity 1000)
    /// 3. Spawn an async task that consumes events from the queue and calls the controller's `handle_watch_response` method
    ///
    /// # Parameters
    ///
    /// * `self` - Must be `Arc<Self>` because it will be cloned and used in async tasks internally
    /// * `controller` - The controller to register, must be `Arc<RwLock<dyn Controller>>`
    /// * `workers` - Maximum number of concurrent processing tasks, uses a semaphore to control the number of simultaneously processed events
    ///
    /// # Returns
    ///
    /// Returns an error if controller initialization fails. Otherwise returns `Ok(())`.
    ///
    /// # Concurrency
    ///
    /// Each controller has its own concurrency limit. For example, if `workers = 10`,
    /// at most 10 events will be processed concurrently. Events exceeding the limit will wait in the queue.
    ///
    /// # Error Handling
    ///
    /// If `handle_watch_response` processing fails, it will automatically retry (up to 5 times with exponential backoff).
    /// After retries fail, an error log is recorded, but it won't affect processing of other events.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let manager = CONTROLLER_MANAGER.clone();
    /// let controller = Arc::new(RwLock::new(MyController::new()));
    ///
    /// // Register controller, allowing up to 10 concurrent processing tasks
    /// manager.register(controller, 10).await?;
    /// ```
    ///
    /// # Notes
    ///
    /// - Controllers must be registered before calling `start_watch`
    /// - Each controller name can only be registered once
    /// - Controllers will run until `shutdown` is called or the manager is dropped
    pub async fn register(
        self: Arc<Self>,
        controller: Arc<RwLock<dyn Controller>>,
        workers: usize,
    ) -> Result<()> {
        controller.write().await.init().await?;
        // create workqueue
        let name = controller.read().await.name().to_string();
        let (tx, mut rx) = mpsc::channel::<ResourceWatchResponse>(1000);

        // register this controller and its queue in the manager
        self.controllers
            .write()
            .await
            .insert(name.clone(), controller.clone());
        self.queues.write().await.insert(name.clone(), tx.clone());
        // initialize inflight set for this controller
        self.inflight
            .write()
            .await
            .insert(name.clone(), HashSet::new());

        // use semaphore to limit the number of concurrent handle watch response workers
        let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));

        // subscribe to the global stop signal so this dispatcher can exit.
        let mut stop_sub = self.stop_tx.subscribe();

        let manager_clone = self.clone();
        let controller_clone = controller.clone();
        let name_clone = name.clone();

        // spawn the dispatcher loop which continuously receives keys from the queue
        // and spawns handle watch response tasks for them.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_sub.changed() => {
                        break;
                    }

                    opt = rx.recv() => {
                        match opt {
                            Some(resp) => {

                                // need to acquire a permit from the semaphore
                                let permit = semaphore.clone().acquire_owned().await.unwrap();

                                let controller = controller_clone.clone();
                                let name = name_clone.clone();
                                let _manager = manager_clone.clone(); // Keep the manager alive for lifetime safety

                                // spawn a new task to handle the watch response
                                tokio::spawn(async move {
                                    if let Err(e) = retry_with_backoff(|| async {
                                        controller.write().await.handle_watch_response(&resp).await?;
                                        Ok(())
                                    }).await {
                                        log::error!(
                                            "controller {} handle watch response {} failed: {:?}",
                                            name, resp.key, e
                                        );
                                    }

                                    // remove from inflight set when done
                                    let mut inflight_map = _manager.inflight.write().await;
                                    if let Some(set) = inflight_map.get_mut(&name) {
                                        set.remove(&resp.key);
                                    }

                                    // release the permit when the task is done.
                                    drop(permit);
                                });
                            }

                            None => break,
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Starts watching Pod and ReplicaSet resources and broadcasts events to controllers that need to watch these resources.
    ///
    /// This method will:
    /// 1. Get a snapshot of all current resources
    /// 2. Send each resource in the snapshot as an `Add` event to corresponding controllers
    /// 3. Start continuous watching from the snapshot revision
    /// 4. Send subsequent `Add`, `Update`, and `Delete` events to corresponding controllers
    ///
    /// # Parameters
    ///
    /// * `self` - Must be `Arc<Self>` because it will be used in background tasks internally
    /// * `store` - XlineStore instance for accessing etcd storage
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` immediately. Actual watching happens in background async tasks.
    ///
    /// # Auto-reconnect
    ///
    /// If the watch connection is lost or errors occur, it will automatically reconnect using an exponential backoff strategy:
    /// - Initial delay: 100ms
    /// - Maximum delay: 30s
    /// - Delay doubles on each retry
    ///
    /// # Event Distribution
    ///
    /// Only controllers that declared they need to watch the corresponding resource type via `watch_resources` will receive events.
    /// For example, only controllers whose `watch_resources` returns a list containing `ResourceKind::Pod` will receive Pod events.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let manager = CONTROLLER_MANAGER.clone();
    /// let store = Arc::new(XlineStore::new(...));
    ///
    /// // Register controllers first
    /// manager.clone().register(my_controller, 10).await?;
    ///
    /// // Then start watching
    /// manager.start_watch(store).await?;
    /// ```
    ///
    /// # Notes
    ///
    /// - Must be called after registering all controllers
    /// - This method spawns two background tasks (watching Pods and ReplicaSets respectively) and does not block
    /// - Watching will continue until the program exits or `shutdown` is called
    pub async fn start_watch(self: Arc<Self>, store: Arc<XlineStore>) -> Result<()> {
        // pods informer with reconnect loop
        let mgr_p = self.clone();
        let store_p = store.clone();

        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_p.pods_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        // broadcast snapshot items to controllers who need to watch pods.
                        for (name, _yaml) in items.into_iter() {
                            let senders = mgr_p.get_senders_by_kind(ResourceKind::Pod).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::Pod,
                                        key: name.clone(),
                                        event: WatchEvent::Add {
                                            yaml: _yaml.clone(),
                                        },
                                    })
                                    .await;
                            }
                        }

                        // start watch from snapshot revision and broadcast events to controllers who need to watch pods.
                        // start watch from rev+1 to avoid re-emitting snapshot items as watch events
                        match store_p.watch_pods(rev + 1).await {
                            Ok((_watcher, mut stream)) => {
                                // reset backoff on successful watch
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/pods/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_p
                                                        .get_senders_by_kind(ResourceKind::Pod)
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::Pod,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!("pod watch stream closed, will reconnect");
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!("pod watch error: {:?}, will reconnect", e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start pod watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot pods: {:?}", e);
                    }
                }

                // backoff before retry
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });

        // services informer with reconnect loop
        let mgr_s = self.clone();
        let store_s = store.clone();
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_s.services_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        // broadcast snapshot items to controllers who need to watch services.
                        for (name, yaml) in items.into_iter() {
                            let senders = mgr_s.get_senders_by_kind(ResourceKind::Service).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::Service,
                                        key: name.clone(),
                                        event: WatchEvent::Add { yaml: yaml.clone() },
                                    })
                                    .await;
                            }
                        }

                        // start watch from snapshot revision and broadcast events
                        // use rev+1 to prevent duplicate Add for snapshot entries
                        match store_s.watch_services(rev + 1).await {
                            Ok((_watcher, mut stream)) => {
                                backoff_ms = 100; // reset backoff
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/services/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "service watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_s
                                                        .get_senders_by_kind(ResourceKind::Service)
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::Service,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!(
                                                "service watch stream closed, will reconnect"
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "service watch error: {:?}, will reconnect",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start service watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot services: {:?}", e);
                    }
                }
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });

        // endpoints informer with reconnect loop
        let mgr_ep = self.clone();
        let store_ep = store.clone();
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_ep.endpoints_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        for (name, yaml) in items.into_iter() {
                            let senders = mgr_ep.get_senders_by_kind(ResourceKind::Endpoint).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::Endpoint,
                                        key: name.clone(),
                                        event: WatchEvent::Add { yaml: yaml.clone() },
                                    })
                                    .await;
                            }
                        }
                        // rev+1 to avoid duplicate Add after snapshot
                        match store_ep.watch_endpoints(rev + 1).await {
                            Ok((_watcher, mut stream)) => {
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/endpoints/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "endpoints watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_ep
                                                        .get_senders_by_kind(ResourceKind::Endpoint)
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::Endpoint,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!(
                                                "endpoints watch stream closed, will reconnect"
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "endpoints watch error: {:?}, will reconnect",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start endpoints watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot endpoints: {:?}", e);
                    }
                }
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });

        // replicasets informer with reconnect loop (use snapshot_with_rev to obtain a starting revision)
        let mgr_rs = self.clone();
        let store_rs = store.clone();
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_rs.replicasets_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        for (name, _yaml) in items.into_iter() {
                            // broadcast snapshot items to controllers who need to watch replicasets.
                            let senders =
                                mgr_rs.get_senders_by_kind(ResourceKind::ReplicaSet).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::ReplicaSet,
                                        key: name.clone(),
                                        event: WatchEvent::Add {
                                            yaml: _yaml.clone(),
                                        },
                                    })
                                    .await;
                            }
                        }

                        // start watch from snapshot revision and broadcast events to controllers who need to watch replicasets.
                        // rev+1 to skip snapshot duplication
                        match store_rs.watch_replicasets(rev + 1).await {
                            Ok((_watcher, mut stream)) => {
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/replicasets/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_rs
                                                        .get_senders_by_kind(
                                                            ResourceKind::ReplicaSet,
                                                        )
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::ReplicaSet,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!(
                                                "replicaset watch stream closed, will reconnect"
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "replicaset watch error: {:?}, will reconnect",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start replicaset watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot replicasets: {:?}", e);
                    }
                }

                // backoff before retry
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });
        // deployments informer with reconnect loop
        let mgr_deploy = self.clone();
        let store_deploy = store.clone();
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_deploy.deployments_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        for (name, _yaml) in items.into_iter() {
                            let senders = mgr_deploy
                                .get_senders_by_kind(ResourceKind::Deployment)
                                .await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::Deployment,
                                        key: name.clone(),
                                        event: WatchEvent::Add {
                                            yaml: _yaml.clone(),
                                        },
                                    })
                                    .await;
                            }
                        }

                        // Start watch from rev+1 to skip snapshot duplication
                        match store_deploy.watch_deployments(rev + 1).await {
                            Ok((_watcher, mut stream)) => {
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/deployments/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_deploy
                                                        .get_senders_by_kind(
                                                            ResourceKind::Deployment,
                                                        )
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::Deployment,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!(
                                                "deployment watch stream closed, will reconnect"
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "deployment watch error: {:?}, will reconnect",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start deployment watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot deployments: {:?}", e);
                    }
                }
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });
        Ok(())
    }

    /// Gracefully shuts down the ControllerManager, stopping all controller processing loops.
    ///
    /// After calling this method:
    /// - All controller dispatcher tasks will receive a stop signal and exit
    /// - Events currently being processed will complete, but new events won't be processed
    /// - Watch tasks will continue running, but events won't be distributed
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Shutdown on program exit
    /// manager.shutdown();
    /// ```
    ///
    /// # Notes
    ///
    /// - This method is idempotent and can be safely called multiple times
    /// - This method will also be automatically called if the manager is dropped
    pub fn shutdown(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Gets all queue senders for controllers that need to watch the specified resource kind.
    ///
    /// This method iterates through all registered controllers, finds those whose `watch_resources`
    /// includes the specified resource kind, and returns their queue senders.
    ///
    /// # Parameters
    ///
    /// * `kind` - The resource kind (e.g., `ResourceKind::Pod`, `ResourceKind::ReplicaSet`)
    ///
    /// # Returns
    ///
    /// Returns a list of queue senders for all controllers that need to watch this resource kind.
    ///
    /// # Internal Use
    ///
    /// This method is used internally by `start_watch` to broadcast events to corresponding controllers.
    async fn get_senders_by_kind(
        &self,
        kind: ResourceKind,
    ) -> Vec<mpsc::Sender<ResourceWatchResponse>> {
        let mut ret = Vec::new();
        for (name, ctrl) in self.controllers.read().await.iter() {
            if ctrl.read().await.watch_resources().contains(&kind)
                && let Some(tx) = self.queues.read().await.get(name)
            {
                ret.push(tx.clone());
            }
        }
        ret
    }
}

impl Default for ControllerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ControllerManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn retry_with_backoff<F, Fut>(mut f: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let mut attempts = 0u32;
    loop {
        match f().await {
            Ok(_) => return Ok(()),
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(e);
                }
                let backoff = 2u64.pow(attempts.min(6)) * 100;
                sleep(Duration::from_millis(backoff)).await;
                continue;
            }
        }
    }
}
