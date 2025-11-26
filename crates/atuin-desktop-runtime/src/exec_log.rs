//! Execution logging for block executions
//!
//! This module provides a SQLite-based logging system for tracking block execution
//! history, including timestamps and outputs.

use log::debug;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};
use std::{fs, path::PathBuf, str::FromStr, time::Duration};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use eyre::Result;

use super::blocks::Block;

/// Database record for a block tracked in the execution log
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecLogBlock {
    /// Database-assigned ID
    pub id: u64,
    /// Block UUID
    pub uuid: Uuid,
}

/// Database record for a single block execution
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecLogEntry {
    /// Database-assigned ID
    pub id: u64,
    /// Foreign key to ExecLogBlock
    pub block_id: u64,
    /// Execution timestamp
    pub timestamp: u64,
    /// Execution output
    pub output: String,
}

/// Messages for interacting with the execution log actor
#[allow(dead_code)]
pub enum ExecLogMessage {
    GetBlock {
        uuid: Uuid,
        reply_to: oneshot::Sender<Result<Option<ExecLogBlock>>>,
    },
    CreateBlock {
        uuid: Uuid,
        reply_to: oneshot::Sender<Result<ExecLogBlock>>,
    },
    GetOrCreateBlock {
        uuid: Uuid,
        reply_to: oneshot::Sender<Result<ExecLogBlock>>,
    },
    LogExecution {
        block_id: Uuid,
        start_time: u64,
        end_time: u64,
        output: String,
        reply_to: oneshot::Sender<Result<()>>,
    },
    GetLastExecutionTime {
        block_id: Uuid,
        reply_to: oneshot::Sender<Result<Option<u64>>>,
    },
}

/// Handle for interacting with the execution log
///
/// Provides async methods for logging block executions and querying execution history.
#[derive(Clone)]
pub struct ExecLogHandle {
    sender: mpsc::Sender<ExecLogMessage>,
}

#[allow(dead_code)]
impl ExecLogHandle {
    /// Create a new execution log handle
    ///
    /// # Arguments
    /// * `path` - Path to the SQLite database file
    pub fn new(path: PathBuf) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(8);

        tokio::spawn(async {
            let mut actor = ExecLog::new(path, receiver)
                .await
                .expect("Failed to create exec log");
            actor.run().await;
        });

        Ok(Self { sender })
    }

    pub async fn get_block(&self, uuid: Uuid) -> Result<Option<ExecLogBlock>> {
        let (reply_to, receiver) = oneshot::channel();
        let msg = ExecLogMessage::GetBlock { uuid, reply_to };

        self.sender.send(msg).await?;
        receiver.await?
    }

    pub async fn create_block(&self, uuid: Uuid) -> Result<ExecLogBlock> {
        let (reply_to, receiver) = oneshot::channel();
        let msg = ExecLogMessage::CreateBlock { uuid, reply_to };

        self.sender.send(msg).await?;
        receiver.await?
    }

    pub async fn get_or_create_block(&self, uuid: Uuid) -> Result<ExecLogBlock> {
        let (reply_to, receiver) = oneshot::channel();
        let msg = ExecLogMessage::GetOrCreateBlock { uuid, reply_to };

        self.sender.send(msg).await?;
        receiver.await?
    }

    pub async fn log_execution(
        &self,
        block: Block,
        start_time: u64,
        end_time: u64,
        output: String,
    ) -> Result<()> {
        let (reply_to, receiver) = oneshot::channel();
        let msg = ExecLogMessage::LogExecution {
            block_id: block.id(),
            start_time,
            end_time,
            output,
            reply_to,
        };

        self.sender.send(msg).await?;
        receiver.await?
    }

    pub async fn get_last_execution_time(&self, block_id: Uuid) -> Result<Option<u64>> {
        let (reply_to, receiver) = oneshot::channel();
        let msg = ExecLogMessage::GetLastExecutionTime { block_id, reply_to };
        self.sender.send(msg).await?;
        receiver.await?
    }
}

pub struct ExecLog {
    pool: SqlitePool,
    receiver: mpsc::Receiver<ExecLogMessage>,
}

impl ExecLog {
    async fn new(path: PathBuf, receiver: mpsc::Receiver<ExecLogMessage>) -> Result<Self> {
        debug!("opening exec_log sqlite database at {path:?}");

        let create = !path.exists();
        if create {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
        }

        let opts = SqliteConnectOptions::from_str(path.as_os_str().to_str().unwrap())?
            .journal_mode(SqliteJournalMode::Wal)
            .optimize_on_close(true, None)
            .synchronous(SqliteSynchronous::Normal)
            .with_regexp()
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs_f64(3.0))
            .connect_with(opts)
            .await?;

        Self::setup_db(&pool).await?;

        Ok(Self { pool, receiver })
    }

    async fn setup_db(pool: &SqlitePool) -> Result<()> {
        sqlx::migrate!("./migrations/exec_log").run(pool).await?;
        Ok(())
    }

    async fn run(&mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                ExecLogMessage::GetBlock { uuid, reply_to } => {
                    let result = self.get_block(uuid).await;
                    let _ = reply_to.send(result);
                }
                ExecLogMessage::CreateBlock { uuid, reply_to } => {
                    let result = self.create_block(uuid).await;
                    let _ = reply_to.send(result);
                }
                ExecLogMessage::GetOrCreateBlock { uuid, reply_to } => {
                    let result = self.get_or_create_block(uuid).await;
                    let _ = reply_to.send(result);
                }
                ExecLogMessage::LogExecution {
                    block_id,
                    start_time,
                    end_time,
                    output,
                    reply_to,
                } => {
                    let result = self
                        .log_execution(block_id, start_time, end_time, output)
                        .await;
                    let _ = reply_to.send(result);
                }
                ExecLogMessage::GetLastExecutionTime { block_id, reply_to } => {
                    let result = self.get_last_execution_time(block_id).await;
                    let _ = reply_to.send(result);
                }
            }
        }
    }

    async fn get_block(&self, uuid: Uuid) -> Result<Option<ExecLogBlock>> {
        let row = sqlx::query("SELECT id, uuid FROM blocks WHERE uuid = ?")
            .bind(uuid.to_string())
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let uuid: String = row.get("uuid");
            let uuid = Uuid::parse_str(&uuid).unwrap();

            Ok(Some(ExecLogBlock {
                id: row.get("id"),
                uuid,
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_block(&self, uuid: Uuid) -> Result<ExecLogBlock> {
        let row = sqlx::query("INSERT INTO blocks (uuid) VALUES (?) RETURNING id")
            .bind(uuid.to_string())
            .fetch_one(&self.pool)
            .await?;

        let id = row.get("id");

        Ok(ExecLogBlock { id, uuid })
    }

    async fn get_or_create_block(&self, uuid: Uuid) -> Result<ExecLogBlock> {
        let block = self.get_block(uuid).await?;

        let block = if let Some(block) = block {
            block
        } else {
            self.create_block(uuid).await?
        };

        Ok(block)
    }

    async fn log_execution(
        &self,
        block_id: Uuid,
        start_time: u64,
        end_time: u64,
        output: String,
    ) -> Result<()> {
        debug!(
            "logging execution for block {:?}, start_time: {}, end_time: {}, output: {}",
            block_id, start_time, end_time, output
        );

        let block = self.get_or_create_block(block_id).await?;

        sqlx::query(
            "INSERT INTO exec_log (block_id, start_time, end_time, output) VALUES (?, ?, ?, ?)",
        )
        .bind(block.id as i64)
        .bind(start_time as i64)
        .bind(end_time as i64)
        .bind(output)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_last_execution_time(&self, block_id: Uuid) -> Result<Option<u64>> {
        let row = sqlx::query("SELECT MAX(exec_log.end_time) as end_time FROM exec_log join blocks on exec_log.block_id = blocks.id WHERE blocks.uuid = ?")
            .bind(block_id.to_string())
            .fetch_one(&self.pool)
            .await?;

        let end_time: Option<i64> = row.get("end_time");
        Ok(end_time.map(|t| t as u64))
    }
}
