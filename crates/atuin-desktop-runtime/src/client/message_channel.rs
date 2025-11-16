use async_trait::async_trait;
use serde::Serialize;

/// Trait for sending messages to the client application
///
/// This abstraction allows different implementations for sending messages
/// from the runtime to the desktop application frontend.
#[async_trait]
pub trait MessageChannel<M: Serialize + Send + Sync>: Send + Sync {
    /// Send a message to the client
    ///
    /// # Arguments
    /// * `message` - The message to send
    ///
    /// # Errors
    /// Returns an error if the message cannot be sent
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<M: Serialize + Send + Sync> MessageChannel<M>
    for fn(M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self(message)
    }
}
