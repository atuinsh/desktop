use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;

use crate::runtime::blocks::{
    handler::{BlockOutput, ExecutionContext, ExecutionHandle},
    BlockBehavior,
};

#[derive(Debug, thiserror::Error)]
pub enum QueryBlockError {
    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Invalid connection string: {0}")]
    InvalidConnectionString(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Cancelled")]
    Cancelled,

    #[error("Generic error: {0}")]
    GenericError(String),
}

impl QueryBlockError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, QueryBlockError::Cancelled)
    }
}

impl From<&str> for QueryBlockError {
    fn from(value: &str) -> Self {
        QueryBlockError::GenericError(value.to_string())
    }
}

#[async_trait]
/// A trait that defines the behavior of a block that executes queries against a remote service
/// (database, monitoring system, etc.). Provides common infrastructure for connection management,
/// query execution, lifecycle events, and cancellation support.
pub(crate) trait QueryBlockBehavior: BlockBehavior + 'static {
    /// The type of the connection (e.g., database pool, HTTP client)
    type Connection: Clone + Send + Sync + 'static;

    /// The type of query results returned by execute_query
    type QueryResult: Serialize + Send + Sync;

    /// The error type for this query block
    type Error: std::error::Error + Send + Sync + From<String> + 'static;

    /// Resolve the query template using the execution context
    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, Self::Error>;

    /// Resolve the connection string/endpoint template using the execution context
    fn resolve_connection_string(&self, context: &ExecutionContext) -> Result<String, Self::Error>;

    /// Connect to the remote service
    async fn connect(connection_string: String) -> Result<Self::Connection, Self::Error>;

    /// Disconnect from the remote service
    async fn disconnect(connection: &Self::Connection) -> Result<(), Self::Error>;

    /// Execute a query against the connection and return results
    async fn execute_query(
        connection: &Self::Connection,
        query: &str,
        context: &ExecutionContext,
    ) -> Result<Vec<Self::QueryResult>, Self::Error>;

    /// Check if an error is a cancellation error
    fn is_cancelled_error(error: &Self::Error) -> bool;

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
                    if Self::is_cancelled_error(&e) {
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
    async fn do_execute(&self, context: ExecutionContext) -> Result<(), Self::Error> {
        let cancellation_receiver = context.handle().cancellation_token.take_receiver();
        let block_id = self.id();
        let connection_string = self.resolve_connection_string(&context)?;
        let query = self.resolve_query(&context)?;

        let context_clone = context.clone();
        let connection_future = Self::connect(connection_string.clone());

        tokio::spawn(async move {
            // Send block started event
            let _ = context_clone.block_started().await;

            let _ = context_clone
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

                tokio::select! {
                    result = connection_future => {
                        match result {
                            Ok(conn) => {
                                let _ = context_clone.send_output(
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
                        let error_msg = QueryBlockError::ConnectionError(message).to_string();
                        return Err(Self::Error::from(error_msg));
                    }
                }
            };

            let _ = context_clone
                .send_output(
                    BlockOutput::builder()
                        .block_id(block_id)
                        .stdout("Executing query...".to_string())
                        .build(),
                )
                .await;

            let context_clone_inner = context_clone.clone();
            let connection_clone = connection.clone();
            let execution_task = async move {
                let results = Self::execute_query(&connection_clone, &query, &context_clone_inner)
                    .await?;

                // Send all results as output
                for result in results {
                    let _ = context_clone_inner
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .object(
                                    serde_json::to_value(result).map_err(|e| {
                                        let error_msg = QueryBlockError::GenericError(format!(
                                            "Unable to serialize query result: {}",
                                            e
                                        ))
                                        .to_string();
                                        Self::Error::from(error_msg)
                                    })?,
                                )
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
                        let _ = Self::disconnect(&connection).await;
                        let error_msg = QueryBlockError::Cancelled.to_string();
                        return Err(Self::Error::from(error_msg));
                    }
                    result = execution_task => {
                        let _ = Self::disconnect(&connection).await;
                        result
                    }
                }
            } else {
                let result = execution_task.await;
                let _ = Self::disconnect(&connection).await;
                result
            };

            if let Err(e) = result {
                return Err(e);
            }

            result
        })
        .await
        .map_err(|e| Self::Error::from(e.to_string()))?
    }
}
