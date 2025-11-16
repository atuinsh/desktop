use crate::context::BlockContext;
use uuid::Uuid;

/// A trait for storing and retrieving block contexts.
#[async_trait::async_trait]
pub trait BlockContextStorage: Send + Sync {
    /// Save a block context to storage.
    async fn save(
        &self,
        document_id: &str,
        block_id: &Uuid,
        context: &BlockContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Load a block context from storage.
    async fn load(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>>;

    /// Delete a block context from storage.
    async fn delete(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Delete all block contexts for a document.
    async fn delete_for_document(
        &self,
        runbook_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
