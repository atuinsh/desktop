use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;

use crate::blocks::BlockBehavior;
use crate::execution::{BlockOutput, ExecutionContext, ExecutionHandle};

pub trait BlockExecutionError {
    /// Create a cancellation error; this is used to indicate that the operation was cancelled by the user.
    fn cancelled() -> Self;

    /// Create a timeout error; this is used to indicate that an operation timed out.
    fn timeout(message: String) -> Self;

    /// Create a serialization error; this is used to indicate that an error occurred while serializing data.
    fn serialization_error(message: String) -> Self;

    /// Whether or not this error is due to cancellation
    fn is_cancelled(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum QueryBlockError {
    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Invalid connection string: {0}")]
    InvalidConnectionString(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Error serializing data: {0}")]
    SerializationError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Cancelled")]
    Cancelled,

    #[error("Generic error: {0}")]
    GenericError(String),
}

impl BlockExecutionError for QueryBlockError {
    fn timeout(_message: String) -> Self {
        QueryBlockError::Timeout
    }

    fn cancelled() -> Self {
        QueryBlockError::Cancelled
    }

    fn serialization_error(message: String) -> Self {
        QueryBlockError::SerializationError(message)
    }

    fn is_cancelled(&self) -> bool {
        matches!(self, QueryBlockError::Cancelled)
    }
}

#[async_trait]
/// A trait that defines the behavior of a block that executes queries against a remote service
/// (database, monitoring system, etc.). Provides common infrastructure for connection management,
/// query execution, lifecycle events, and cancellation support.
pub trait QueryBlockBehavior: BlockBehavior + 'static {
    /// The type of the connection (e.g., database pool, HTTP client)
    type Connection: Clone + Send + Sync + 'static;

    /// The type of query results returned by execute_query
    type QueryResult: Serialize + Send + Sync;

    /// The error type for the block; must implement [`BlockExecutionError`]
    type Error: std::error::Error + Send + Sync + BlockExecutionError;

    /// Resolve the query template using the execution context
    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, Self::Error>;

    /// Resolve the connection string/endpoint template using the execution context
    fn resolve_connection_string(&self, context: &ExecutionContext) -> Result<String, Self::Error>;

    /// Connect to the remote service
    async fn connect(&self, connection_string: String) -> Result<Self::Connection, Self::Error>;

    /// Disconnect from the remote service
    async fn disconnect(&self, connection: &Self::Connection) -> Result<(), Self::Error>;

    /// Execute a query against the connection and return results
    async fn execute_query(
        &self,
        connection: &Self::Connection,
        query: &str,
        context: &ExecutionContext,
    ) -> Result<Vec<Self::QueryResult>, Self::Error>;

    /// Execute the block. Creates an execution handle and manages all lifecycle events.
    /// This is the main entry point that handles the full execution lifecycle.
    async fn execute_query_block(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let handle = context.handle();

        tokio::spawn(async move {
            match self.do_execute(context.clone()).await {
                Ok(_) => {
                    let _ = context.block_finished(None, true).await;
                }
                Err(e) => {
                    if e.is_cancelled() {
                        let _ = context.block_cancelled().await;
                    } else {
                        let _ = context.block_failed(e.to_string()).await;
                    }
                }
            }
        });

        Ok(Some(handle))
    }

    /// Internal method that performs the actual execution with all lifecycle management
    async fn do_execute(&self, context: ExecutionContext) -> Result<(), Self::Error>
    where
        Self: Sized + Sync,
    {
        let cancellation_receiver = context.handle().cancellation_token.take_receiver();
        let block_id = self.id();
        let query = self.resolve_query(&context)?;
        let connection_string = self.resolve_connection_string(&context)?;

        // Send block started event
        let _ = context.block_started().await;

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(block_id)
                    .stdout("Connecting...".to_string())
                    .build(),
            )
            .await;

        // Connect with timeout
        let connection = {
            let timeout = tokio::time::sleep(Duration::from_secs(10));
            let connection_future = self.connect(connection_string);

            tokio::select! {
                result = connection_future => {
                    match result {
                        Ok(conn) => {
                            let _ = context.send_output(
                                BlockOutput::builder()
                                    .block_id(block_id)
                                    .stdout("Connected successfully".to_string())
                                    .build(),
                            ).await;
                            conn
                        },
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                _ = timeout => {
                    let message = "Connection timed out after 10 seconds.".to_string();
                    return Err(Self::Error::timeout(message));
                }
            }
        };

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(block_id)
                    .stdout("Executing query...".to_string())
                    .build(),
            )
            .await;

        let execution_task = async {
            let results = self.execute_query(&connection, &query, &context).await?;

            // Send all results as output
            for result in results {
                let _ = context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(block_id)
                            .object(serde_json::to_value(result).map_err(|e| {
                                Self::Error::serialization_error(format!(
                                    "Unable to serialize query result: {}",
                                    e
                                ))
                            })?)
                            .build(),
                    )
                    .await;
            }

            Ok::<(), Self::Error>(())
        };

        // Execute with cancellation support
        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    let _ = self.disconnect(&connection).await;
                    return Err(Self::Error::cancelled());
                }
                result = execution_task => {
                    let _ = self.disconnect(&connection).await;
                    result
                }
            }
        } else {
            let result = execution_task.await;
            let _ = self.disconnect(&connection).await;
            result
        };

        result
    }
}
