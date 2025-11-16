#[cfg(test)]
use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

/// A trait for providing local values for blocks.
#[async_trait]
pub trait LocalValueProvider: Send + Sync {
    async fn get_block_local_value(
        &self,
        block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;
}

#[cfg(test)]
pub(crate) struct MemoryBlockLocalValueProvider {
    values: HashMap<String, String>,
}

#[cfg(test)]
impl MemoryBlockLocalValueProvider {
    #[cfg(test)]
    pub fn new(values: Vec<(String, String)>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl LocalValueProvider for MemoryBlockLocalValueProvider {
    async fn get_block_local_value(
        &self,
        _block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.values.get(property_name).cloned())
    }
}
