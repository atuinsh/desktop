use super::handler::{BlockHandler, BlockOutput, ExecutionContext, ExecutionHandle};
use super::handlers::ScriptHandler;
use super::Block;
use crate::runtime::workflow::event::WorkflowEvent;
use tauri::ipc::Channel;
use tokio::sync::broadcast;

pub struct BlockRegistry;

impl BlockRegistry {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_block(
        &self,
        block: &Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: tauri::AppHandle,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        match block {
            Block::Script(script) => {
                ScriptHandler
                    .execute(
                        script.clone(),
                        context,
                        event_sender,
                        output_channel,
                        app_handle,
                    )
                    .await
            }
            Block::Terminal(_terminal) => {
                // TODO: Implement TerminalHandler
                Err("Terminal handler not yet implemented".into())
            }
            Block::Postgres(_postgres) => {
                // TODO: Implement PostgresHandler
                Err("Postgres handler not yet implemented".into())
            }
            Block::Http(_http) => {
                // TODO: Implement HttpHandler
                Err("HTTP handler not yet implemented".into())
            }
            Block::Prometheus(_prometheus) => {
                // TODO: Implement PrometheusHandler
                Err("Prometheus handler not yet implemented".into())
            }
            Block::Clickhouse(_clickhouse) => {
                // TODO: Implement ClickhouseHandler
                Err("Clickhouse handler not yet implemented".into())
            }
            Block::Mysql(_mysql) => {
                // TODO: Implement MysqlHandler
                Err("MySQL handler not yet implemented".into())
            }
            Block::SQLite(_sqlite) => {
                // TODO: Implement SQLiteHandler
                Err("SQLite handler not yet implemented".into())
            }
        }
    }
}

