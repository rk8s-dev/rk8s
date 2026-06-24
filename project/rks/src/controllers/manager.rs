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

#[derive(Clone, Copy)]
struct ResourceWatchSpec {
    kind: ResourceKind,
    key_prefix: &'static str,
}

impl ResourceWatchSpec {
    const fn new(kind: ResourceKind, key_prefix: &'static str) -> Self {
        Self { kind, key_prefix }
    }

    async fn snapshot(self, store: &XlineStore) -> Result<(Vec<(String, String)>, i64)> {
        match self.kind {
            ResourceKind::Pod => store.pods_snapshot_with_rev().await,
            ResourceKind::Service => store.services_snapshot_with_rev().await,
            ResourceKind::Endpoint => store.endpoints_snapshot_with_rev().await,
            ResourceKind::ReplicaSet => store.replicasets_snapshot_with_rev().await,
            ResourceKind::Deployment => store.deployments_snapshot_with_rev().await,
            ResourceKind::Job => store.jobs_snapshot_with_rev().await,
            ResourceKind::Unknown => unreachable!("unsupported watch resource"),
        }
    }

    async fn watch(
        self,
        store: &XlineStore,
        start_rev: i64,
    ) -> Result<(etcd_client::Watcher, etcd_client::WatchStream)> {
        match self.kind {
            ResourceKind::Pod => store.watch_pods(start_rev).await,
            ResourceKind::Service => store.watch_services(start_rev).await,
            ResourceKind::Endpoint => store.watch_endpoints(start_rev).await,
            ResourceKind::ReplicaSet => store.watch_replicasets(start_rev).await,
            ResourceKind::Deployment => store.watch_deployments(start_rev).await,
            ResourceKind::Job => store.watch_jobs(start_rev).await,
            ResourceKind::Unknown => unreachable!("unsupported watch resource"),
        }
    }
}

const RESOURCE_WATCH_SPECS: [ResourceWatchSpec; 6] = [
    ResourceWatchSpec::new(ResourceKind::Pod, "/registry/pods/"),
    ResourceWatchSpec::new(ResourceKind::Service, "/registry/services/"),
    ResourceWatchSpec::new(ResourceKind::Endpoint, "/registry/endpoints/"),
    ResourceWatchSpec::new(ResourceKind::ReplicaSet, "/registry/replicasets/"),
    ResourceWatchSpec::new(ResourceKind::Deployment, "/registry/deployments/"),
    ResourceWatchSpec::new(ResourceKind::Job, "/registry/jobs/"),
];

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
                                // Check if already in-flight before processing
                                {
                                    let mut inflight_map = manager_clone.inflight.write().await;
                                    if let Some(set) = inflight_map.get_mut(&name_clone) {
                                        if set.contains(&resp.key) {
                                            log::debug!(
                                                "Skipping duplicate event for {} (already in-flight)",
                                                resp.key
                                            );
                                            continue;
                                        }
                                        set.insert(resp.key.clone());
                                    }
                                }

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

    /// Starts watching supported resources and broadcasts events to controllers that declared them.
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
    /// - This method spawns one background task per watched resource kind and does not block
    /// - Watching will continue until the program exits or `shutdown` is called
    pub async fn start_watch(self: Arc<Self>, store: Arc<XlineStore>) -> Result<()> {
        for spec in RESOURCE_WATCH_SPECS {
            self.clone().spawn_resource_watch(store.clone(), spec);
        }
        Ok(())
    }

    fn spawn_resource_watch(self: Arc<Self>, store: Arc<XlineStore>, spec: ResourceWatchSpec) {
        tokio::spawn(async move {
            self.run_resource_watch_loop(store, spec).await;
        });
    }

    async fn run_resource_watch_loop(
        self: Arc<Self>,
        store: Arc<XlineStore>,
        spec: ResourceWatchSpec,
    ) {
        let mut backoff_ms = 100u64;
        loop {
            match spec.snapshot(&store).await {
                Ok((items, rev)) => {
                    self.broadcast_snapshot_items(spec.kind, items).await;

                    // Watch from rev + 1 so snapshot items are not replayed as watch events.
                    match spec.watch(&store, rev + 1).await {
                        Ok((_watcher, mut stream)) => {
                            backoff_ms = 100;
                            self.process_watch_stream(spec, &mut stream).await;
                        }
                        Err(e) => {
                            log::error!("failed to start {} watch: {:?}", spec.kind, e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("failed to snapshot {}: {:?}", spec.kind, e);
                }
            }

            sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(30_000);
        }
    }

    async fn broadcast_snapshot_items(&self, kind: ResourceKind, items: Vec<(String, String)>) {
        for (key, yaml) in items {
            self.broadcast_resource_event(kind, key, WatchEvent::Add { yaml })
                .await;
        }
    }

    async fn process_watch_stream(
        &self,
        spec: ResourceWatchSpec,
        stream: &mut etcd_client::WatchStream,
    ) {
        loop {
            match stream.message().await {
                Ok(Some(resp)) => {
                    for event in resp.events() {
                        let Some((key, watch_event)) = Self::parse_watch_event(spec, event) else {
                            continue;
                        };
                        self.broadcast_resource_event(spec.kind, key, watch_event)
                            .await;
                    }
                }
                Ok(None) => {
                    log::info!("{} watch stream closed, will reconnect", spec.kind);
                    break;
                }
                Err(e) => {
                    log::error!("{} watch error: {:?}, will reconnect", spec.kind, e);
                    break;
                }
            }
        }
    }

    fn parse_watch_event(
        spec: ResourceWatchSpec,
        event: &etcd_client::Event,
    ) -> Option<(String, WatchEvent)> {
        let kv = event.kv()?;
        let key = Self::strip_watch_key(spec.key_prefix, kv.key());
        let watch_event = match event.event_type() {
            etcd_client::EventType::Put => {
                if let Some(prev_kv) = event.prev_kv() {
                    WatchEvent::Update {
                        old_yaml: String::from_utf8_lossy(prev_kv.value()).to_string(),
                        new_yaml: String::from_utf8_lossy(kv.value()).to_string(),
                    }
                } else {
                    WatchEvent::Add {
                        yaml: String::from_utf8_lossy(kv.value()).to_string(),
                    }
                }
            }
            etcd_client::EventType::Delete => {
                if let Some(prev_kv) = event.prev_kv() {
                    WatchEvent::Delete {
                        yaml: String::from_utf8_lossy(prev_kv.value()).to_string(),
                    }
                } else {
                    log::warn!(
                        "{} watch delete event missing prev_kv for key {}",
                        spec.kind,
                        key
                    );
                    return None;
                }
            }
        };
        Some((key, watch_event))
    }

    fn strip_watch_key(prefix: &str, key: &[u8]) -> String {
        let key = String::from_utf8_lossy(key);
        key.strip_prefix(prefix).unwrap_or(key.as_ref()).to_string()
    }

    async fn broadcast_resource_event(&self, kind: ResourceKind, key: String, event: WatchEvent) {
        let senders = self.get_senders_by_kind(kind).await;
        for sender in senders {
            let _ = sender
                .send(ResourceWatchResponse {
                    kind,
                    key: key.clone(),
                    event: event.clone(),
                })
                .await;
        }
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
