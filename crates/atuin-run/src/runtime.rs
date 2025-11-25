use atuin_desktop_runtime::{
    client::{DocumentBridgeMessage, LocalValueProvider, MessageChannel},
    context::{BlockContext, BlockContextStorage},
    events::{EventBus, GCEvent},
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct NullEventBus;

#[async_trait::async_trait]
impl EventBus for NullEventBus {
    async fn emit(
        &self,
        _event: GCEvent,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

pub struct NullDocumentBridge;

#[async_trait::async_trait]
impl MessageChannel<DocumentBridgeMessage> for NullDocumentBridge {
    async fn send(
        &self,
        _message: DocumentBridgeMessage,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}
pub struct ChannelDocumentBridge {
    sender: mpsc::Sender<DocumentBridgeMessage>,
}

impl ChannelDocumentBridge {
    pub fn new(sender: mpsc::Sender<DocumentBridgeMessage>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl MessageChannel<DocumentBridgeMessage> for ChannelDocumentBridge {
    async fn send(
        &self,
        message: DocumentBridgeMessage,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sender.send(message).await.map_err(|e| e.into())
    }
}

pub struct TempNullLocalValueProvider;

#[async_trait::async_trait]
impl LocalValueProvider for TempNullLocalValueProvider {
    async fn get_block_local_value(
        &self,
        _block_id: Uuid,
        _property_name: &str,
    ) -> std::result::Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

pub struct TempNullContextStorage;

#[async_trait::async_trait]
impl BlockContextStorage for TempNullContextStorage {
    async fn save(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
        _context: &BlockContext,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn load(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
    ) -> std::result::Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn delete(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn delete_for_document(
        &self,
        _runbook_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}
