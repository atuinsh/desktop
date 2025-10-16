use super::handler::{BlockHandler, BlockOutput, ExecutionContext, ExecutionHandle};
use super::handlers::{
    ClickhouseHandler, MySQLHandler, PostgresHandler, PrometheusHandler, SQLiteHandler,
    ScriptHandler, TerminalHandler,
};
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
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        match block {
            Block::Script(script) => {
                ScriptHandler
                    .execute(script.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::Terminal(terminal) => {
                TerminalHandler
                    .execute(terminal.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::Postgres(postgres) => {
                PostgresHandler
                    .execute(postgres.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::Http(_http) => {
                // TODO: Implement HttpHandler
                Err("HTTP handler not yet implemented".into())
            }
            Block::Prometheus(prometheus) => {
                PrometheusHandler
                    .execute(prometheus.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::Clickhouse(clickhouse) => {
                ClickhouseHandler
                    .execute(clickhouse.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::Mysql(mysql) => {
                MySQLHandler
                    .execute(mysql.clone(), context, event_sender, output_channel)
                    .await
            }
            Block::SQLite(sqlite) => {
                SQLiteHandler
                    .execute(sqlite.clone(), context, event_sender, output_channel)
                    .await
            }
        }
    }
}
