// An actor for managing all ptys

use async_trait::async_trait;
use bytes::Bytes;
use eyre::Result;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

use uuid::Uuid;

use crate::pty::PtyMetadata;

#[async_trait]
pub trait PtyLike {
    fn metadata(&self) -> PtyMetadata;
    async fn kill_child(&self) -> Result<()>;
    async fn send_bytes(&self, data: Bytes) -> Result<()>;
    async fn resize(&self, rows: u16, cols: u16) -> Result<()>;
}

pub enum PtyStoreMessage {
    AddPty {
        pty: Box<dyn PtyLike + Send>,
    },
    RemovePty {
        id: Uuid,
    },
    GetPtyMeta {
        id: Uuid,
        reply_to: oneshot::Sender<Option<PtyMetadata>>,
    },
    ListPtyMeta {
        reply_to: oneshot::Sender<Vec<PtyMetadata>>,
    },
    ListPtyForRunbook {
        runbook: Uuid,
        reply_to: oneshot::Sender<Vec<PtyMetadata>>,
    },
    WritePty {
        id: Uuid,
        data: Bytes,
    },
    ResizePty {
        id: Uuid,
        rows: u16,
        cols: u16,
    },
    Len {
        reply_to: oneshot::Sender<usize>,
    },
}

#[derive(Clone, Debug)]
pub struct PtyStoreHandle {
    sender: mpsc::Sender<PtyStoreMessage>,
}

impl PtyStoreHandle {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let mut actor = PtyStore::new(receiver);

        tokio::spawn(async move { actor.run().await });

        Self { sender }
    }

    pub async fn add_pty(
        &self,
        pty: Box<dyn PtyLike + Send>,
    ) -> Result<(), mpsc::error::SendError<PtyStoreMessage>> {
        let msg = PtyStoreMessage::AddPty { pty };

        self.sender.send(msg).await
    }

    pub async fn remove_pty(
        &self,
        id: Uuid,
    ) -> Result<(), mpsc::error::SendError<PtyStoreMessage>> {
        let msg = PtyStoreMessage::RemovePty { id };

        self.sender.send(msg).await
    }

    pub async fn len(&self) -> Result<usize, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = PtyStoreMessage::Len { reply_to: sender };

        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn write_pty(
        &self,
        id: Uuid,
        data: Bytes,
    ) -> Result<(), mpsc::error::SendError<PtyStoreMessage>> {
        let msg = PtyStoreMessage::WritePty { id, data };

        self.sender.send(msg).await
    }

    pub async fn resize_pty(
        &self,
        id: Uuid,
        rows: u16,
        cols: u16,
    ) -> Result<(), mpsc::error::SendError<PtyStoreMessage>> {
        let msg = PtyStoreMessage::ResizePty { id, rows, cols };

        self.sender.send(msg).await
    }

    pub async fn get_pty_meta(
        &self,
        id: Uuid,
    ) -> Result<Option<PtyMetadata>, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = PtyStoreMessage::GetPtyMeta {
            id,
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn list_pty_meta(&self) -> Result<Vec<PtyMetadata>, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = PtyStoreMessage::ListPtyMeta { reply_to: sender };

        let _ = self.sender.send(msg).await;
        receiver.await
    }

    pub async fn list_pty_for_runbook(
        &self,
        runbook: Uuid,
    ) -> Result<Vec<PtyMetadata>, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = PtyStoreMessage::ListPtyForRunbook {
            runbook,
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;
        receiver.await
    }
}

impl Default for PtyStoreHandle {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct PtyStore {
    pub receiver: mpsc::Receiver<PtyStoreMessage>,
    pty_sessions: HashMap<Uuid, Box<dyn PtyLike + Send>>,
}

impl PtyStore {
    pub fn new(receiver: mpsc::Receiver<PtyStoreMessage>) -> Self {
        Self {
            receiver,
            pty_sessions: HashMap::new(),
        }
    }

    async fn run(&mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;

            tracing::debug!(
                "PtyStore Message, store length: {}",
                self.pty_sessions.len()
            );
        }
    }

    async fn handle_message(&mut self, message: PtyStoreMessage) {
        match message {
            PtyStoreMessage::AddPty { pty } => self.add_pty(pty),
            PtyStoreMessage::RemovePty { id } => self.remove_pty(id).await,
            PtyStoreMessage::WritePty { id, data } => self.write_pty(id, data).await,
            PtyStoreMessage::ResizePty { id, rows, cols } => self.pty_resize(id, rows, cols).await,
            PtyStoreMessage::Len { reply_to } => {
                let len = self.pty_sessions.len();
                reply_to.send(len).unwrap();
            }
            PtyStoreMessage::GetPtyMeta { id, reply_to } => {
                let pty = self.pty_sessions.get(&id);
                reply_to.send(pty.map(|pty| pty.metadata())).unwrap();
            }
            PtyStoreMessage::ListPtyMeta { reply_to } => {
                let pty_metas = self
                    .pty_sessions
                    .values()
                    .map(|pty| pty.metadata())
                    .collect();
                reply_to.send(pty_metas).unwrap();
            }
            PtyStoreMessage::ListPtyForRunbook { runbook, reply_to } => {
                let pty_metas = self
                    .pty_sessions
                    .values()
                    .filter(|pty| pty.metadata().runbook == runbook)
                    .map(|pty| pty.metadata())
                    .collect();
                reply_to.send(pty_metas).unwrap();
            }
        }
    }

    fn add_pty(&mut self, pty: Box<dyn PtyLike + Send>) {
        self.pty_sessions.insert(pty.metadata().pid, pty);
    }

    async fn remove_pty(&mut self, id: Uuid) {
        let pty = self.pty_sessions.remove(&id);

        if let Some(pty) = pty {
            let metadata = pty.metadata();
            tracing::info!("Killing pty: {}", metadata.pid);

            if let Err(e) = pty.kill_child().await {
                tracing::debug!(
                    "Failed to kill PTY child {}: {} (likely already closed)",
                    metadata.pid,
                    e
                );
            }
        }
    }

    async fn write_pty(&mut self, id: Uuid, data: Bytes) {
        if let Some(pty) = self.pty_sessions.get_mut(&id) {
            if let Err(e) = pty.send_bytes(data).await {
                tracing::debug!("Failed to send bytes to PTY {id}: {e} (likely session closed)");
            }
        }
    }

    async fn pty_resize(&mut self, id: Uuid, rows: u16, cols: u16) {
        if let Some(pty) = self.pty_sessions.get_mut(&id) {
            if let Err(e) = pty.resize(rows, cols).await {
                tracing::debug!("Failed to resize PTY {id}: {e} (likely session closed)");
            }
        }
    }
}
