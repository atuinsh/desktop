use eyre::Result;
use minijinja::Environment;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Emitter;
use tauri::{async_runtime::RwLock, AppHandle};
use tokio::{
    process::Child,
    sync::{broadcast, mpsc},
};
use uuid::Uuid;

use crate::{
    runtime::{
        events::GCEvent,
        exec_log::ExecLogHandle,
        pty_store::PtyStoreHandle,
        ssh_pool::SshPoolHandle,
        workflow::{
            event::{WorkflowCommand, WorkflowEvent},
            executor::ExecutorHandle,
        },
    },
    shared_state::SharedStateHandle,
    sqlite::DbInstances,
    workspaces::manager::WorkspaceManager,
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
    pub db_instances: DbInstances,

    // Shared state
    shared_state: Mutex<Option<SharedStateHandle>>,

    // File-based workspaces
    pub workspaces: Arc<tokio::sync::Mutex<Option<WorkspaceManager>>>,

    executor: Mutex<Option<ExecutorHandle>>,
    event_sender: Mutex<Option<broadcast::Sender<WorkflowEvent>>>,

    // Grand Central event system
    pub gc_event_sender: Mutex<Option<mpsc::UnboundedSender<GCEvent>>>,
    pub event_receiver: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedReceiver<GCEvent>>>>,

    // the second rwlock could probs be a mutex
    // i cba it works fine
    pub child_processes: Arc<RwLock<HashMap<uuid::Uuid, Arc<RwLock<Child>>>>>,

    /// Map a runbook id, to a Jinja environment
    /// In the future it may make sense to map to our own abstracted
    /// environment state, but atm this is fine.
    pub template_state: RwLock<HashMap<Uuid, Arc<Environment<'static>>>>,

    // Persisted to the keychain, but cached here so that
    // we don't keep asking the user for keychain access.
    // Map of user -> password
    // Service is hardcoded
    pub runbooks_api_token: RwLock<HashMap<String, String>>,

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
    pub block_executions:
        Arc<RwLock<HashMap<Uuid, crate::runtime::blocks::handler::ExecutionHandle>>>,
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
            db_instances: DbInstances::new(app_path.clone(), dev_prefix.clone()),
            shared_state: Mutex::new(None),
            workspaces: Arc::new(tokio::sync::Mutex::new(None)),
            executor: Mutex::new(None),
            event_sender: Mutex::new(None),
            gc_event_sender: Mutex::new(None),
            event_receiver: Arc::new(tokio::sync::Mutex::new(None)),
            child_processes: Default::default(),
            template_state: Default::default(),
            runbooks_api_token: Default::default(),
            runbook_output_variables: Default::default(),
            block_executions: Default::default(),
            dev_prefix,
            app_path,
            use_hub_updater_service,
        }
    }
    pub async fn init(&self, app: &AppHandle) -> Result<()> {
        let path = if let Some(ref prefix) = self.dev_prefix {
            self.app_path.join(format!("{prefix}_exec_log.db"))
        } else {
            self.app_path.join("exec_log.db")
        };

        self.db_instances.init().await?;

        // For some reason we cannot spawn the exec log task before the state is managed. Annoying.
        let exec_log = ExecLogHandle::new(path).expect("Failed to boot exec log");
        self.exec_log.lock().unwrap().replace(exec_log);

        let pty_store = PtyStoreHandle::new_with_app(app.clone());
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
        let (event_sender, mut event_receiver) = tokio::sync::broadcast::channel(24);
        let (cmd_sender, mut cmd_receiver) = mpsc::channel(8);

        let executor = ExecutorHandle::new(event_sender.clone(), cmd_sender);

        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            println!("starting executor command loop");

            while let Some(event) = cmd_receiver.recv().await {
                match event {
                    WorkflowCommand::RunBlock { id } => {
                        println!("emitting start block event {id}");
                        app_clone
                            .emit("start-block", id)
                            .expect("Failed to emit start block event");
                    }
                    WorkflowCommand::StopBlock { id } => {
                        println!("emitting stop block event {id}");
                        app_clone
                            .emit("stop-block", id)
                            .expect("Failed to emit stop block event");
                    }
                }
            }
        });

        let app_clone = app.clone();
        let executor_clone = executor.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = event_receiver.recv().await {
                match event {
                    WorkflowEvent::BlockStarted { id } => {
                        println!("block {id} started");
                        app_clone
                            .emit("block-started", id)
                            .expect("Failed to emit block started event");
                    }
                    WorkflowEvent::BlockFinished { id } => {
                        println!("block {id} finished");
                        app_clone
                            .emit("block-finished", id)
                            .expect("Failed to emit block finished event");
                    }
                    WorkflowEvent::WorkflowStarted { id } => {
                        println!("workflow {id} started");
                        app_clone
                            .emit("workflow-started", id)
                            .expect("Failed to emit workflow started event");
                    }
                    WorkflowEvent::WorkflowFinished { id } => {
                        println!("workflow {id} finished");
                        executor_clone.stop_workflow(id).await;
                        app_clone
                            .emit("workflow-finished", id)
                            .expect("Failed to emit workflow finished event");
                    }
                }
            }
        });

        self.executor.lock().unwrap().replace(executor);
        self.event_sender.lock().unwrap().replace(event_sender);

        // Initialize Grand Central event system
        let (gc_sender, gc_receiver) = mpsc::unbounded_channel::<GCEvent>();
        self.gc_event_sender.lock().unwrap().replace(gc_sender);
        *self.event_receiver.lock().await = Some(gc_receiver);

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
}
