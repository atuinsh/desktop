use std::{collections::HashMap, sync::Arc};

use tauri::ipc::Channel;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::{
    ai::{
        fsm::State as FsmState,
        session::{AISession, SessionEvent, SessionHandle},
        storage::{AISessionStorage, SerializedAISessionV1},
        types::{
            AIMessage, AIToolCall, BlockInfo, LLMToolsEvent, SessionConfig, SessionInfo,
            SessionKind,
        },
    },
    secret_cache::SecretCache,
};

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("Session {0} not found")]
    SessionNotFound(Uuid),

    #[error("Storage error: {0}")]
    StorageError(#[from] crate::ai::storage::StorageError),

    #[error("Session error: {0}")]
    SessionError(#[from] crate::ai::session::AISessionError),
}

/// Data that needs to be replayed to the frontend when a session is restored.
struct PendingReplay {
    fsm_state: FsmState,
    history: Vec<AIMessage>,
    pending_tools: Vec<AIToolCall>,
}

/// Manages the creation and destruction of AI sessions.
pub struct AISessionManager {
    secret_cache: Arc<SecretCache>,
    storage: Arc<AISessionStorage>,

    sessions: Arc<RwLock<HashMap<Uuid, SessionHandle>>>,
    channels: Arc<RwLock<HashMap<Uuid, Channel<SessionEvent>>>>,

    pending_replays: Arc<RwLock<HashMap<Uuid, PendingReplay>>>,

    /// Tracks session info for list_sessions and LLMToolsEvent broadcasts.
    session_infos: Arc<RwLock<HashMap<Uuid, SessionInfo>>>,

    /// Broadcast channel for LLM Tools window events.
    llmtools_tx: broadcast::Sender<LLMToolsEvent>,
}

impl AISessionManager {
    pub fn new(secret_cache: Arc<SecretCache>, storage: Arc<AISessionStorage>) -> Self {
        // Create broadcast channel with reasonable capacity for LLM Tools subscribers
        let (llmtools_tx, _) = broadcast::channel(256);

        Self {
            secret_cache,
            storage,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            pending_replays: Arc::new(RwLock::new(HashMap::new())),
            session_infos: Arc::new(RwLock::new(HashMap::new())),
            llmtools_tx,
        }
    }

    /// Get a session handle for a session ID.
    pub async fn get_handle(&self, session_id: Uuid) -> Option<SessionHandle> {
        self.sessions.read().await.get(&session_id).cloned()
    }

    /// Destroy a session and clean up resources.
    pub async fn destroy(&self, session_id: Uuid) {
        self.sessions.write().await.remove(&session_id);
        self.channels.write().await.remove(&session_id);
        self.pending_replays.write().await.remove(&session_id);
        self.session_infos.write().await.remove(&session_id);

        // Broadcast session destroyed event (ignore errors if no subscribers)
        let _ = self
            .llmtools_tx
            .send(LLMToolsEvent::SessionDestroyed { session_id });

        log::info!("Destroyed AI session {}", session_id);
    }

    /// Subscribe to events from a session.
    pub async fn subscribe(
        &self,
        session_id: Uuid,
        channel: Channel<SessionEvent>,
    ) -> Result<(), ManagerError> {
        // Verify session exists
        if !self.sessions.read().await.contains_key(&session_id) {
            return Err(ManagerError::SessionNotFound(session_id));
        }

        self.channels
            .write()
            .await
            .insert(session_id, channel.clone());

        // Send initial state and replay data
        let replay = self.pending_replays.write().await.remove(&session_id);
        self.send_initial_state(session_id, &channel, replay).await;

        log::debug!("Frontend subscribed to session {}", session_id);
        Ok(())
    }

    /// List all active sessions with their info.
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.session_infos.read().await.values().cloned().collect()
    }

    /// Subscribe to LLM Tools events (session creation, destruction, and session events).
    pub fn subscribe_llmtools(&self) -> broadcast::Receiver<LLMToolsEvent> {
        self.llmtools_tx.subscribe()
    }

    pub async fn create_chat_session(
        &self,
        runbook_id: Uuid,
        block_infos: Vec<BlockInfo>,
        config: SessionConfig,
        restore_previous: bool,
    ) -> Result<SessionHandle, ManagerError> {
        let kind = SessionKind::AssistantChat {
            runbook_id,
            block_infos,
        };

        self.create_session(kind, config, restore_previous).await
    }

    pub async fn create_generator_session(
        &self,
        runbook_id: Uuid,
        block_infos: Vec<BlockInfo>,
        current_document: serde_json::Value,
        insert_after: Uuid,
        config: SessionConfig,
    ) -> Result<SessionHandle, ManagerError> {
        let kind = SessionKind::InlineBlockGeneration {
            runbook_id,
            block_infos,
            current_document,
            insert_after,
            is_initial_generation: true,
        };

        self.create_session(kind, config, false).await
    }

    async fn create_session(
        &self,
        kind: SessionKind,
        config: SessionConfig,
        restore_previous: bool,
    ) -> Result<SessionHandle, ManagerError> {
        let existing = if restore_previous && kind.persists_state() {
            self.storage
                .find_most_recent_for_runbook(&kind.runbook_id())
                .await?
        } else {
            None
        };

        let (output_tx, output_rx) = mpsc::channel::<SessionEvent>(32);

        let (session, handle, replay_data, session_info) = if let Some(saved) = existing {
            log::info!(
                "Restoring AI session {} for runbook {}",
                saved.id,
                kind.runbook_id()
            );

            let replay = self.extract_replay_data(&saved)?;
            let info = SessionInfo::from_session_kind(saved.id, &saved.kind);
            let (session, handle) = AISession::from_saved(
                saved,
                output_tx,
                self.secret_cache.clone(),
                self.storage.clone(),
            )?;

            (session, handle, Some(replay), info)
        } else {
            log::info!("Creating new AI session for runbook {}", kind.runbook_id());

            // Capture session info before kind is moved
            let info = SessionInfo::from_session_kind(Uuid::new_v4(), &kind);
            let (session, handle) = AISession::new(
                kind,
                config,
                output_tx,
                self.secret_cache.clone(),
                self.storage.clone(),
            )?;

            // Update info with actual session id
            let info = SessionInfo {
                id: session.id(),
                ..info
            };

            (session, handle, None, info)
        };

        let session_id = session.id();

        if let Some(replay) = replay_data {
            if !replay.history.is_empty() || !replay.pending_tools.is_empty() {
                self.pending_replays
                    .write()
                    .await
                    .insert(session_id, replay);
            }
        }

        // Store session info for list_sessions
        self.session_infos
            .write()
            .await
            .insert(session_id, session_info.clone());

        self.sessions
            .write()
            .await
            .insert(session_id, handle.clone());

        tokio::spawn(session.run());

        self.spawn_event_forwarder(session_id, output_rx);

        // Broadcast session created event (ignore errors if no subscribers)
        let _ = self
            .llmtools_tx
            .send(LLMToolsEvent::SessionCreated { info: session_info });

        Ok(handle)
    }

    fn extract_replay_data(
        &self,
        saved: &SerializedAISessionV1,
    ) -> Result<PendingReplay, ManagerError> {
        let replay = PendingReplay {
            fsm_state: saved.agent_state.clone(),
            history: saved
                .agent_context
                .conversation
                .iter()
                .map(|msg| AIMessage::from(msg.clone()))
                .collect(),
            pending_tools: saved
                .agent_context
                .pending_tools
                .values()
                .cloned()
                .map(AIToolCall::from)
                .collect(),
        };

        Ok(replay)
    }

    fn spawn_event_forwarder(&self, session_id: Uuid, mut output_rx: mpsc::Receiver<SessionEvent>) {
        let channels = self.channels.clone();
        let sessions = self.sessions.clone();
        let session_infos = self.session_infos.clone();
        let llmtools_tx = self.llmtools_tx.clone();

        tokio::spawn(async move {
            while let Some(event) = output_rx.recv().await {
                // Broadcast to LLM Tools window (ignore errors if no subscribers)
                let _ = llmtools_tx.send(LLMToolsEvent::SessionEvent {
                    session_id,
                    event: event.clone(),
                });

                // Forward to the frontend channel for this specific session
                let channels = channels.read().await;
                if let Some(channel) = channels.get(&session_id) {
                    if let Err(e) = channel.send(event) {
                        log::error!("Failed to send event to frontend: {}", e);
                        break;
                    }
                }
            }

            // Session ended, clean up
            log::debug!("Session {session_id} output channel closed, cleaning up");
            sessions.write().await.remove(&session_id);
            channels.write().await.remove(&session_id);
            session_infos.write().await.remove(&session_id);

            // Broadcast session destroyed event (ignore errors if no subscribers)
            let _ = llmtools_tx.send(LLMToolsEvent::SessionDestroyed { session_id });
        });
    }

    async fn send_initial_state(
        &self,
        session_id: Uuid,
        channel: &Channel<SessionEvent>,
        replay: Option<PendingReplay>,
    ) {
        let fsm_state = replay
            .as_ref()
            .map(|r| r.fsm_state.clone())
            .unwrap_or(FsmState::Idle);

        let history = replay
            .as_ref()
            .map(|r| r.history.clone())
            .unwrap_or_default();

        let pending_tool_calls = replay.map(|r| r.pending_tools.clone()).unwrap_or_default();

        log::debug!(
            "Sending state {:?}, {} history messages, and {} pending tool calls for session {}",
            fsm_state,
            history.len(),
            pending_tool_calls.len(),
            session_id
        );

        if let Err(e) = channel.send(SessionEvent::StateChanged { state: fsm_state }) {
            log::error!("Failed to send state to frontend: {}", e);
        }

        if let Err(e) = channel.send(SessionEvent::History {
            messages: history,
            pending_tool_calls,
        }) {
            log::error!("Failed to send history to frontend: {}", e);
        }
    }
}
