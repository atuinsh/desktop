use eyre::Result;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::ipc::Channel;
use tauri::{async_runtime::RwLock, AppHandle};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::{
    ai::session::{SessionEvent, SessionHandle},
    secret_cache::{KeychainSecretStorage, KvDbSecretStorage, SecretCache},
};
use crate::{
    shared_state::SharedStateHandle, sqlite::DbInstances, workspaces::manager::WorkspaceManager,
};
use atuin_desktop_runtime::{
    document::DocumentHandle,
    events::GCEvent,
    exec_log::ExecLogHandle,
    execution::ExecutionHandle,
    pty::PtyStoreHandle,
    ssh::SshPoolHandle,
    workflow::{ExecutorHandle, WorkflowEvent},
};

pub(crate) struct AtuinState {
    // Mutex for that interior mutability
    // We can also just clone these
    // This is pretty gross. But I've found that trying to do too much inside of the state init
    // leads to a really sad Tauri.
    // So
    // 1. Manage the state, provide simple values
    // 2. Initialize the state in the init function
    // Annoying that it needs to be done in two steps, but ok.
    pty_store: Mutex<Option<PtyStoreHandle>>,
    exec_log: Mutex<Option<ExecLogHandle>>,
    ssh_pool: Mutex<Option<SshPoolHandle>>,
    pub db_instances: Arc<DbInstances>,

    // Shared state
    shared_state: Mutex<Option<SharedStateHandle>>,

    // File-based workspaces
    pub workspaces: Arc<tokio::sync::Mutex<Option<WorkspaceManager>>>,

    pub serial_executions: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,

    executor: Mutex<Option<ExecutorHandle>>,
    event_sender: Mutex<Option<broadcast::Sender<WorkflowEvent>>>,

    // Grand Central event system
    pub gc_event_sender: Mutex<Option<mpsc::UnboundedSender<GCEvent>>>,
    pub event_receiver: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedReceiver<GCEvent>>>>,
    pub gc_frontend_channel: tokio::sync::Mutex<Option<Channel<GCEvent>>>,

    // The prefix to use for SQLite and local storage in development mode
    pub dev_prefix: Option<String>,

    pub app_path: PathBuf,

    // Whether to use the Hub updater service
    pub use_hub_updater_service: bool,

    // Map of runbook -> output variable -> output value
    // All strings
    // I'd like to store the output of all executions in a local sqlite next, but
    // to start lets just store the latest value
    pub runbook_output_variables: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,

    // Map of block execution id -> execution handle for cancellation
    pub block_executions: Arc<RwLock<HashMap<Uuid, ExecutionHandle>>>,

    // Map of document handles per runbook
    pub documents: Arc<RwLock<HashMap<String, Arc<DocumentHandle>>>>,

    // AI session handles for sending events to sessions
    pub ai_sessions: Arc<RwLock<HashMap<Uuid, SessionHandle>>>,

    // AI session event channels for sending events to frontend (per session)
    pub ai_session_channels: Arc<RwLock<HashMap<Uuid, Channel<SessionEvent>>>>,

    // Secret cache for storing secrets (backed by keychain in prod, KV DB in dev)
    secret_cache: Mutex<Option<Arc<SecretCache>>>,
}

impl AtuinState {
    pub fn new(
        dev_prefix: Option<String>,
        app_path: PathBuf,
        use_hub_updater_service: bool,
    ) -> Self {
        Self {
            pty_store: Mutex::new(None),
            exec_log: Mutex::new(None),
            ssh_pool: Mutex::new(None),
            db_instances: Arc::new(DbInstances::new(app_path.clone(), dev_prefix.clone())),
            shared_state: Mutex::new(None),
            workspaces: Arc::new(tokio::sync::Mutex::new(None)),
            serial_executions: Default::default(),
            executor: Mutex::new(None),
            event_sender: Mutex::new(None),
            gc_event_sender: Mutex::new(None),
            event_receiver: Arc::new(tokio::sync::Mutex::new(None)),
            gc_frontend_channel: tokio::sync::Mutex::new(None),
            runbook_output_variables: Default::default(),
            block_executions: Default::default(),
            documents: Default::default(),
            ai_sessions: Default::default(),
            ai_session_channels: Default::default(),
            dev_prefix,
            app_path,
            use_hub_updater_service,
            secret_cache: Mutex::new(None),
        }
    }
    pub async fn init(&self, _app: &AppHandle) -> Result<()> {
        let path = if let Some(ref prefix) = self.dev_prefix {
            self.app_path.join(format!("{prefix}_exec_log.db"))
        } else {
            self.app_path.join("exec_log.db")
        };

        self.db_instances.init().await?;
        self.db_instances
            .add_migrator("context", sqlx::migrate!("./migrations/context"))
            .await?;
        self.db_instances
            .add_migrator("ai", sqlx::migrate!("./migrations/ai"))
            .await?;

        // For some reason we cannot spawn the exec log task before the state is managed. Annoying.
        let exec_log = ExecLogHandle::new(path).expect("Failed to boot exec log");
        self.exec_log.lock().unwrap().replace(exec_log);

        let pty_store = PtyStoreHandle::new();
        self.pty_store.lock().unwrap().replace(pty_store);

        let ssh_pool = SshPoolHandle::new();
        self.ssh_pool.lock().unwrap().replace(ssh_pool);

        let shared_state =
            SharedStateHandle::new(self.db_instances.get_pool("shared_state").await?).await;
        self.shared_state.lock().unwrap().replace(shared_state);

        let workspaces = WorkspaceManager::new();
        self.workspaces.lock().await.replace(workspaces);

        // New receivers are created by calling .subscribe() on the sender
        // Hence, we pass in the sender and not a receiver
        // This is a BROADCAST channel, not a normal mpsc!
        // TODO: handle broadcast channel lag
        //
        // Used by the executor for sending workflow events,
        // but not currently used in the application after the 0.2.0 runtime update.
        let (event_sender, mut _event_receiver) = tokio::sync::broadcast::channel(24);
        let (cmd_sender, mut _cmd_receiver) = mpsc::channel(8);

        // NOTE: The executor is currently not doing anything post runtime update.
        // It may still do so in the future for dependency-based workflows.
        let executor = ExecutorHandle::new(event_sender.clone(), cmd_sender);

        self.executor.lock().unwrap().replace(executor);
        self.event_sender.lock().unwrap().replace(event_sender);

        // Initialize Grand Central event system
        let (gc_sender, gc_receiver) = mpsc::unbounded_channel::<GCEvent>();
        self.gc_event_sender.lock().unwrap().replace(gc_sender);
        *self.event_receiver.lock().await = Some(gc_receiver);

        // Initialize secret cache with appropriate storage backend
        let secret_cache = if let Some(ref prefix) = self.dev_prefix {
            // Dev mode: use KV DB storage
            let pool = self.db_instances.get_pool("kv").await?;
            let storage = Arc::new(KvDbSecretStorage::new(prefix.clone(), pool));
            SecretCache::new(storage)
        } else {
            // Production: use keychain storage
            let storage = Arc::new(KeychainSecretStorage);
            SecretCache::new(storage)
        };
        self.secret_cache
            .lock()
            .unwrap()
            .replace(Arc::new(secret_cache));

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        let shared_state = self.shared_state.lock().unwrap().take();

        if let Some(shared_state) = shared_state {
            shared_state.shutdown().await?;
        }

        if let Some(mut workspaces) = self.workspaces.lock().await.take() {
            workspaces.shutdown().await;
        }

        self.db_instances.shutdown().await?;

        Ok(())
    }

    pub fn exec_log(&self) -> ExecLogHandle {
        if let Some(exec_log) = &self.exec_log.lock().unwrap().as_ref() {
            (*exec_log).clone()
        } else {
            panic!("Exec log not initialized");
        }
    }

    pub fn pty_store(&self) -> PtyStoreHandle {
        if let Some(pty_store) = &self.pty_store.lock().unwrap().as_ref() {
            (*pty_store).clone()
        } else {
            panic!("Pty store not initialized");
        }
    }

    pub fn ssh_pool(&self) -> SshPoolHandle {
        if let Some(ssh_pool) = &self.ssh_pool.lock().unwrap().as_ref() {
            (*ssh_pool).clone()
        } else {
            panic!("SSH pool not initialized");
        }
    }

    pub fn shared_state(&self) -> SharedStateHandle {
        if let Some(shared_state) = self.shared_state.lock().unwrap().as_ref() {
            shared_state.clone()
        } else {
            panic!("Shared state not initialized");
        }
    }

    pub fn executor(&self) -> ExecutorHandle {
        if let Some(executor) = self.executor.lock().unwrap().as_ref() {
            (*executor).clone()
        } else {
            panic!("Executor not initialized");
        }
    }

    pub fn event_sender(&self) -> broadcast::Sender<WorkflowEvent> {
        if let Some(event_sender) = self.event_sender.lock().unwrap().as_ref() {
            (*event_sender).clone()
        } else {
            panic!("Event sender not initialized");
        }
    }

    pub fn gc_event_sender(&self) -> mpsc::UnboundedSender<GCEvent> {
        if let Some(gc_event_sender) = self.gc_event_sender.lock().unwrap().as_ref() {
            gc_event_sender.clone()
        } else {
            panic!("GC event sender not initialized");
        }
    }

    pub fn secret_cache(&self) -> Arc<SecretCache> {
        if let Some(secret_cache) = self.secret_cache.lock().unwrap().as_ref() {
            secret_cache.clone()
        } else {
            panic!("Secret cache not initialized");
        }
    }
}
