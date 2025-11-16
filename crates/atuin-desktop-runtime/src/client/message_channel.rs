use async_trait::async_trait;
use serde::Serialize;

#[async_trait]
pub trait MessageChannel<M: Serialize + Send + Sync>: Send + Sync {
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
