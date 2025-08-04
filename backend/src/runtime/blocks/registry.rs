use std::collections::HashMap;

use super::handler::{BlockHandler, BlockOutput, BoxFuture, ContextProvider, ExecutionContext, ExecutionHandle};
use super::handlers::{DirectoryHandler, EnvironmentHandler, ScriptHandler, SshConnectHandler};
use super::Block;
use crate::runtime::workflow::event::WorkflowEvent;
use tauri::ipc::Channel;
use tokio::sync::broadcast;

// Type-erased trait for storing handlers
trait AnyBlockHandler: Send + Sync {
    fn execute_any<'a>(
        &'a self,
        block: Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: tauri::AppHandle,
    ) -> BoxFuture<'a, Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>>>;
}

// Type-erased trait for storing context providers
trait AnyContextProvider: Send + Sync {
    fn apply_context_any(
        &self,
        block: &Block,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

// Blanket implementation for BlockHandler
impl<H> AnyBlockHandler for H
where
    H: BlockHandler,
    H::Block: 'static + Clone,
{
    fn execute_any<'a>(
        &'a self,
        block: Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: tauri::AppHandle,
    ) -> BoxFuture<'a, Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>>> {
        Box::pin(async move {
            // Extract the correct block type
            let typed_block = extract_block::<H::Block>(&block)?;
            let handle = self.execute(typed_block, context, event_sender, output_channel, app_handle).await?;
            Ok(handle)
        })
    }
}

// Helper function to extract typed block from enum
fn extract_block<T: 'static>(block: &Block) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    T: Clone,
{
    // This is a bit hacky but works for our use case
    // In a real implementation, we'd match on the block type
    match block {
        Block::Script(s) => {
            let any = Box::new(s.clone()) as Box<dyn std::any::Any>;
            any.downcast::<T>()
                .map(|b| *b)
                .map_err(|_| "Block type mismatch".into())
        }
        _ => Err("Unsupported block type for extraction".into()),
    }
}

pub struct BlockRegistry {
    handlers: HashMap<&'static str, Box<dyn AnyBlockHandler>>,
    context_providers: HashMap<&'static str, Box<dyn AnyContextProvider>>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
            context_providers: HashMap::new(),
        };

        // Register all handlers
        registry.register_handler(ScriptHandler);
        // TODO: Add more handlers as they're implemented
        // registry.register_handler(TerminalHandler);
        // registry.register_handler(PostgresHandler);

        // Register context providers
        registry.register_context_provider(DirectoryHandler);
        registry.register_context_provider(EnvironmentHandler);
        registry.register_context_provider(SshConnectHandler);

        registry
    }

    pub fn register_handler<H>(&mut self, handler: H)
    where
        H: BlockHandler + 'static,
        H::Block: Clone,
    {
        self.handlers
            .insert(handler.block_type(), Box::new(handler));
    }

    pub fn register_context_provider<P>(&mut self, provider: P)
    where
        P: ContextProvider + 'static,
    {
        // We need to wrap the provider to make it type-erased
        struct ProviderWrapper<P: ContextProvider> {
            provider: P,
        }

        impl<P: ContextProvider> AnyContextProvider for ProviderWrapper<P> {
            fn apply_context_any(
                &self,
                _block: &Block,
                _context: &mut ExecutionContext,
                ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {                // For now, we'll handle this in the context builder
                // This is a placeholder
                Ok(())
            }
        }

        self.context_providers.insert(
            provider.block_type(),
            Box::new(ProviderWrapper { provider }),
        );
    }

    pub fn get_handler(&self, block_type: &str) -> Option<&Box<dyn AnyBlockHandler>> {
        self.handlers.get(block_type)
    }

    pub fn get_context_provider(&self, block_type: &str) -> Option<&Box<dyn AnyContextProvider>> {
        self.context_providers.get(block_type)
    }

    pub async fn execute_block(
        &self,
        block: &Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: tauri::AppHandle,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let block_type = match block {
            Block::Script(_) => "script",
            Block::Terminal(_) => "terminal",
            Block::Postgres(_) => "postgres",
            Block::Http(_) => "http",
            Block::Prometheus(_) => "prometheus",
            Block::Clickhouse(_) => "clickhouse",
            Block::Mysql(_) => "mysql",
            Block::SQLite(_) => "sqlite",
        };

        if let Some(handler) = self.get_handler(block_type) {
            handler.execute_any(block.clone(), context, event_sender, output_channel, app_handle).await
        } else {
            Err(format!("No handler registered for block type: {}", block_type).into())
        }
    }
}