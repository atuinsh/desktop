use std::sync::Arc;

use atuin_desktop_runtime::document::{DocumentError, DocumentHandle};
use tokio::sync::mpsc;

use crate::{
    runbooks::Runbook,
    runtime::{
        ChannelDocumentBridge, NullEventBus, TempNullContextStorage, TempNullLocalValueProvider,
    },
};

type Result<T> = std::result::Result<T, ExecutorError>;

#[derive(thiserror::Error, Debug)]
pub enum ExecutorError {
    #[error("Runtime reported a document error: {0}")]
    RuntimeDocumentError(#[from] DocumentError),
}

pub struct Executor {
    runbook: Runbook,
    interactive: bool,
}

impl Executor {
    pub fn new(runbook: Runbook, interactive: bool) -> Self {
        Self {
            runbook,
            interactive,
        }
    }

    // TODO: find variables / inputs that need setting
    pub async fn execute(&self) -> Result<()> {
        let (sender, mut receiver) = mpsc::channel(16);

        let document = DocumentHandle::new(
            self.runbook.id.to_string(),
            Arc::new(NullEventBus),
            Arc::new(ChannelDocumentBridge::new(sender)),
            Some(Box::new(TempNullLocalValueProvider)),
            Some(Box::new(TempNullContextStorage)),
        );

        document.put_document(self.runbook.content.clone()).await?;

        Ok(())
    }
}
