# Atuin Desktop Backend Execution System

This document provides a comprehensive overview of how the Atuin Desktop backend executes runbook blocks, manages execution context, and handles cancellation.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Core Components](#core-components)
- [Execution Flow](#execution-flow)
- [Context Management](#context-management)
- [Cancellation & Error Handling](#cancellation--error-handling)
- [Developer Guide](#developer-guide)
- [Examples & Patterns](#examples--patterns)

## Architecture Overview

The Atuin Desktop execution system is built around a **block-based architecture** where runbooks are composed of individual blocks that can be executed independently or in sequence.

### Key Design Principles

1. **Separation of Concerns**: Block data structures are separate from execution logic
2. **Async Execution**: All block execution is asynchronous and cancellable
3. **Context Isolation**: Each execution has its own context that can be modified by context blocks
4. **Type Safety**: Strong typing with compile-time guarantees where possible
5. **Extensibility**: Easy to add new block types and handlers

### Block Types

There are two main categories of blocks:

#### **Execution Blocks**
Blocks that perform actual work (run commands, make HTTP requests, query databases):
- `Script` - Execute shell commands
- `Terminal` - Interactive terminal sessions
- `Http` - HTTP requests
- `Postgres`, `MySQL`, `SQLite`, `Clickhouse` - Database queries
- `Prometheus` - Metrics queries

#### **Context Blocks**
Blocks that modify the execution environment for subsequent blocks (via `passive_context` method):
- `Directory` - Change working directory
- `LocalDirectory` - Change working directory (not persisted in Runbook)
- `Environment` - Set environment variables
- `Host` - Select host for SSH connection
- `SshConnect` - Configure SSH connection
- `Var` - Set document-level variables
- `LocalVar` - Set variable (not persisted in Runbook)

## Core Components

### BlockBehavior Trait

The `BlockBehavior` trait defines how blocks behave:

```rust
#[async_trait]
pub trait BlockBehavior: Sized + Send + Sync {
    fn into_block(self) -> Block;

    fn id(&self) -> Uuid;

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        block_local_value_provider: Option<&dyn BlockLocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}
```

**Key Methods:**
- `into_block()` - Converts the block into the generic `Block` enum
- `id()` - Returns the unique identifier (UUID) for this block instance
- `passive_context()` - For context blocks, returns the context this block provides (evaluated on document changes)
- `execute()` - For execution blocks, performs the actual block execution (returns immediately with a handle)

### ExecutionContext

The `ExecutionContext` contains all the information needed to execute a block:

```rust
pub struct ExecutionContext {
    pub block_id: Uuid,                       // ID of the block being executed
    pub runbook_id: Uuid,                     // Which runbook this execution belongs to
    pub document_handle: Arc<DocumentHandle>, // Handle to the document actor for context updates
    pub context_resolver: Arc<ContextResolver>, // Resolves templates and provides context values
    output_channel: Option<Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>>, // For sending output to frontend
    workflow_event_sender: broadcast::Sender<WorkflowEvent>, // For sending workflow events
    pub ssh_pool: Option<SshPoolHandle>,      // SSH connection pool (if available)
    pub pty_store: Option<PtyStoreHandle>,    // PTY store for terminal blocks (if available)
    pub gc_event_bus: Option<Arc<dyn EventBus>>, // Grand Central event bus
    handle: ExecutionHandle,                  // Handle for this execution
}
```

**Key Fields:**
- `block_id` - The UUID of the block currently being executed
- `context_resolver` - Wrapped in Arc, provides access to variables, cwd, env vars, ssh_host via `resolve_template()` and getter methods
- `document_handle` - Used to update block's active or passive context (e.g., storing output variables)
- `handle` - The execution handle for this block execution

**Helper Methods:**
The ExecutionContext provides several convenience methods to simplify block implementations:

- **Lifecycle management:**
  - `block_started()` - Marks block as started and sends appropriate events
  - `block_finished(exit_code, success)` - Marks block as finished
  - `block_failed(error)` - Marks block as failed with an error message
  - `block_cancelled()` - Marks block as cancelled

- **Context management:**
  - `update_passive_context(block_id, update_fn)` - Update a block's passive context
  - `update_active_context(block_id, update_fn)` - Update a block's active context
  - `clear_active_context(block_id)` - Clear a block's active context

- **Communication:**
  - `send_output(message)` - Send output to the frontend via the document bridge
  - `emit_workflow_event(event)` - Emit a workflow event
  - `emit_gc_event(event)` - Emit a Grand Central event
  - `prompt_client(prompt)` - Prompt the client for input (e.g., password, confirmation)

- **Cancellation:**
  - `cancellation_token()` - Get the cancellation token
  - `cancellation_receiver()` - Get a receiver for cancellation signals

**Lifecycle:**
1. **Created** by `Document::build_execution_context()` when a block execution is requested
2. **Contains** a snapshot of the cumulative context from all blocks above the executing block
3. **Used** by execution blocks to resolve templates and access context values
4. **Discarded** after execution completes

### ExecutionHandle

The `ExecutionHandle` represents a running or completed block execution:

```rust
pub struct ExecutionHandle {
    pub id: Uuid,                                    // Unique execution ID
    pub block_id: Uuid,                              // ID of the block being executed
    pub cancellation_token: CancellationToken,       // For graceful cancellation
    pub status: Arc<RwLock<ExecutionStatus>>,        // Current execution status
    pub output_variable: Option<String>,             // Output variable name (if any)
    pub prompt_callbacks: Arc<Mutex<HashMap<Uuid, oneshot::Sender<ClientPromptResult>>>>, // Callbacks for client prompts
}
```

**Purpose:**
- **Async Management**: Allows tracking of long-running operations
- **Cancellation**: Provides mechanism to stop execution gracefully
- **Status Monitoring**: Frontend can poll execution status
- **Output Capture**: Links execution results to variables for use in subsequent blocks
- **Client Prompts**: Manages callbacks for prompting the client for input (e.g., passwords, confirmations)

**Helper Methods:**
- `new(block_id)` - Creates a new handle with a unique execution ID
- `set_running()` - Updates status to Running
- `set_success()` - Updates status to Success
- `set_failed(error)` - Updates status to Failed with an error message
- `set_cancelled()` - Updates status to Cancelled

### ExecutionStatus

Tracks the current state of a block execution:

```rust
pub enum ExecutionStatus {
    Running,
    Success,            // Block completed successfully
    Failed(String),     // Contains error message
    Cancelled,
}
```

**Note:** In earlier versions, `Success` contained the output value, but this is now stored in the block's active context instead, allowing it to persist across app restarts.

### Context System Components

**BlockContext**
A type-safe storage for context values:

```rust
pub struct BlockContext {
    entries: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
```

BlockContext stores typed values (like `DocumentVar`, `DocumentCwd`, etc.) that can be retrieved type-safely. Context items are serializable using `typetag::serde`, enabling persistence to disk.

Each block has two independent contexts:
- **Passive context** - Evaluated when the document changes, provides context for blocks below it (e.g., working directory, environment variables). Not persisted to disk.
- **Active context** - Set during execution, stores output variables and execution results. Persisted to disk via `ContextStorage`, allowing state to survive app restarts.

**BlockWithContext**
Wrapper that pairs a block with its contexts:

```rust
pub struct BlockWithContext {
    block: Block,
    passive_context: BlockContext,
    active_context: BlockContext,
}
```

When constructing a `BlockWithContext`, you can optionally provide an active context (e.g., loaded from disk). If not provided, an empty context is used.

**ContextResolver**
Resolves templates and provides access to cumulative context from blocks above:

```rust
pub struct ContextResolver {
    vars: HashMap<String, String>,      // Variables from Var blocks
    cwd: String,                        // Working directory from Directory blocks
    env_vars: HashMap<String, String>,  // Environment variables from Environment blocks
    ssh_host: Option<String>,           // SSH host from Host/SshConnect blocks
}
```

**ResolvedContext**
A serializable snapshot of context for the frontend:

```rust
pub struct ResolvedContext {
    pub variables: HashMap<String, String>,
    pub cwd: String,
    pub env_vars: HashMap<String, String>,
    pub ssh_host: Option<String>,
}
```

### Document System

**Document**
Manages all blocks in a runbook with their contexts:

```rust
pub struct Document {
    id: String,
    blocks: Vec<BlockWithContext>,  // Blocks with their passive/active contexts
    document_bridge: Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>,
    // ...
}
```

**DocumentHandle & DocumentActor**
Actor-based system for thread-safe document operations:
- `DocumentHandle` - Public API for interacting with a document
- `DocumentActor` - Background actor that processes commands and manages document state
- Operations: update document, rebuild contexts, start/complete execution, update contexts

### Query Block Architecture

The execution system provides a specialized architecture for blocks that execute queries against remote services (databases, monitoring systems, etc.). This architecture promotes code reuse and consistent behavior across different query-executing blocks.

**QueryBlockBehavior Trait**

The `QueryBlockBehavior` trait provides a common pattern for blocks that:
- Connect to remote services (databases, APIs, monitoring systems)
- Execute queries/requests
- Return structured results
- Support cancellation and timeout handling

```rust
#[async_trait]
pub(crate) trait QueryBlockBehavior: BlockBehavior + 'static {
    /// The type of the connection (e.g., database pool, HTTP client)
    type Connection: Clone + Send + Sync + 'static;

    /// The type of query results returned by execute_query
    type QueryResult: Serialize + Send + Sync;

    /// The error type for the block
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

    /// Execute the block with full lifecycle management (provided by default)
    async fn execute_query_block(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        // Default implementation handles:
        // - Creating execution handle
        // - Spawning background task
        // - Connection with timeout
        // - Query execution
        // - Result serialization and output
        // - Cancellation support
        // - Lifecycle events (started, finished, failed, cancelled)
    }
}
```

**Key Features:**
- **Instance methods**: All methods take `&self`, allowing access to block-specific data (e.g., `self.period` for Prometheus)
- **Type safety**: Associated types for Connection, QueryResult, and Error preserve type information
- **Default implementation**: `execute_query_block()` provides common execution logic (connection, timeout, cancellation, lifecycle events)
- **Extensible**: Blocks only need to implement the core methods specific to their service

**Example - Prometheus Block:**
```rust
impl QueryBlockBehavior for Prometheus {
    type Connection = (Client, String, PrometheusTimeRange);
    type QueryResult = PrometheusQueryResult;
    type Error = PrometheusBlockError;

    async fn connect(&self, context: &ExecutionContext) -> Result<Self::Connection, Self::Error> {
        let endpoint = self.resolve_connection_string(context)?;
        let client = ClientBuilder::new().timeout(Duration::from_secs(30)).build()?;

        // Can access self.period directly - instance method!
        let time_range = PrometheusTimeRange::from_period(&self.period);

        Ok((client, endpoint, time_range))
    }

    async fn execute_query(
        &self,
        connection: &Self::Connection,
        query: &str,
        _context: &ExecutionContext,
    ) -> Result<Vec<Self::QueryResult>, Self::Error> {
        let (client, endpoint, time_range) = connection;
        // Execute Prometheus range query
        // Parse response
        // Return structured results
    }
}
```

**SqlBlockBehavior Trait**

SQL blocks extend `QueryBlockBehavior` with SQL-specific functionality through a blanket implementation:

```rust
pub(crate) trait SqlBlockBehavior: BlockBehavior + 'static {
    type Pool: Clone + Send + Sync + 'static;

    /// SQL-specific methods
    fn dialect() -> Box<dyn Dialect>;
    fn is_query(statement: &Statement) -> bool;

    async fn execute_sql_query(
        &self,
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError>;

    async fn execute_sql_statement(
        &self,
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError>;
}

// Blanket implementation automatically implements QueryBlockBehavior for all SqlBlockBehavior types
#[async_trait]
impl<T> QueryBlockBehavior for T
where
    T: SqlBlockBehavior,
{
    type Connection = T::Pool;
    type QueryResult = SqlBlockExecutionResult;
    type Error = SqlBlockError;

    async fn execute_query(
        &self,
        connection: &Self::Connection,
        query: &str,
        context: &ExecutionContext,
    ) -> Result<Vec<SqlBlockExecutionResult>, SqlBlockError> {
        // SQL-specific logic:
        // - Parse SQL (handles multiple semicolon-separated statements)
        // - Distinguish queries (SELECT) from statements (INSERT/UPDATE/DELETE)
        // - Execute each query/statement appropriately
        // - Return results for all queries
    }
}
```

**Benefits:**
- SQL blocks (Postgres, MySQL, ClickHouse, SQLite) automatically get:
  - Connection management
  - Timeout handling
  - Cancellation support
  - Lifecycle event handling
  - SQL parsing and multiple query support
- Only need to implement database-specific connection and execution logic

**BlockExecutionError Trait**

To preserve error type information while providing infrastructure error handling, all query block errors must implement `BlockExecutionError`:

```rust
pub trait BlockExecutionError: std::error::Error + Send + Sync {
    /// Check if this error represents a cancellation
    fn is_cancelled(&self) -> bool;

    /// Factory methods for common infrastructure errors
    fn timeout() -> Self;
    fn cancelled() -> Self;
    fn serialization_error(msg: String) -> Self;
}
```

**Example Implementations:**

```rust
// SQL blocks
impl BlockExecutionError for SqlBlockError {
    fn is_cancelled(&self) -> bool {
        matches!(self, SqlBlockError::Cancelled)
    }

    fn timeout() -> Self {
        SqlBlockError::ConnectionError("Connection timed out".to_string())
    }

    fn cancelled() -> Self {
        SqlBlockError::Cancelled
    }

    fn serialization_error(msg: String) -> Self {
        SqlBlockError::GenericError(msg)
    }
}

// Prometheus block
impl BlockExecutionError for PrometheusBlockError {
    fn is_cancelled(&self) -> bool {
        matches!(self, PrometheusBlockError::Cancelled)
    }

    fn timeout() -> Self {
        PrometheusBlockError::ConnectionError("Connection timed out".to_string())
    }

    fn cancelled() -> Self {
        PrometheusBlockError::Cancelled
    }

    fn serialization_error(msg: String) -> Self {
        PrometheusBlockError::SerializationError(msg)
    }
}
```

**Trait Hierarchy:**

```
BlockBehavior (base trait for all blocks)
    ↓
QueryBlockBehavior (query-executing blocks)
    ↓
    ├─ Prometheus (implements QueryBlockBehavior directly)
    │
    └─ SqlBlockBehavior (SQL-specific extension)
           ↓
           ├─ Postgres
           ├─ MySQL
           ├─ ClickHouse
           └─ SQLite
```

**Architecture Benefits:**
- **Code reuse**: Common execution logic implemented once in `QueryBlockBehavior::execute_query_block()`
- **Type safety**: Errors preserve semantic meaning (not converted to strings)
- **Consistency**: All query blocks behave uniformly (timeouts, cancellation, lifecycle)
- **Extensibility**: Easy to add new query-executing blocks
- **Maintainability**: SQL-specific logic isolated in `SqlBlockBehavior`

### Supporting Traits

The execution system defines several key traits that enable extensibility and abstraction:

**ClientMessageChannel**
Trait for sending messages to the client (frontend):

```rust
pub trait ClientMessageChannel<M: Serialize + Send + Sync>: Send + Sync {
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

This trait abstracts the communication channel to the client, allowing the runtime to work in different environments (Tauri, CLI, web server, etc.). In the Tauri app, this is implemented using Tauri's IPC channel system.

**BlockLocalValueProvider**
Trait for accessing block-local values that are not persisted in the runbook:

```rust
pub trait BlockLocalValueProvider: Send + Sync {
    async fn get_block_local_value(
        &self,
        block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;
}
```

This trait allows blocks to access values that are stored locally (e.g., in the Tauri app's state) but not in the runbook document itself. For example, the local directory block uses this to access the set directory, which is managed by the frontend but not persisted in the runbook JSON.

**ContextStorage**
Trait for persisting block context to disk:

```rust
pub trait ContextStorage: Send + Sync {
    async fn save(
        &self,
        document_id: &str,
        block_id: &Uuid,
        context: &BlockContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn load(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>>;

    async fn delete(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn delete_for_document(
        &self,
        runbook_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

This trait enables the runtime to persist block execution state (active context) to disk, allowing context to survive app restarts. In the Tauri app, the context is serialized to JSON and stored in the application data directory. Context items must implement the `typetag::serde` trait to be serializable.

## Execution Flow

### 1. Document Opening

When a runbook is opened in the frontend, the `open_document` command is called:

```rust
#[tauri::command]
pub async fn open_document(
    state: State<'_, AtuinState>,
    document_id: String,
    document: Vec<serde_json::Value>,
    document_bridge: Channel<DocumentBridgeMessage>,
) -> Result<(), String>
```

This:
1. Creates a `DocumentHandle` and spawns a `DocumentActor` for the runbook
2. Stores the document in `state.documents`
3. Parses and flattens the document into a list of `Block`s
4. Loads active contexts from disk (if they exist) for each block
5. Builds initial passive contexts for all blocks

### 2. Document Updates

When the document changes (blocks added/removed/modified), `update_document` is called:

```rust
#[tauri::command]
pub async fn update_document(
    state: State<'_, AtuinState>,
    document_id: String,
    document_content: Vec<serde_json::Value>,
) -> Result<(), String>
```

The document actor:
1. Identifies which blocks changed, were added, or removed
2. Determines the earliest index where context needs rebuilding
3. Rebuilds passive contexts for affected blocks
4. Sends context updates to the frontend via the document bridge

### 3. Block Execution Request

When the frontend requests block execution via `execute_block`:

```rust
#[tauri::command]
pub async fn execute_block(
    state: State<'_, AtuinState>,
    block_id: String,
    runbook_id: String,
) -> Result<String, String>
```

### 4. Context Snapshot & Execution

The execution flow:

1. **Get the document** from state by runbook_id
2. **Create execution context** via `document.start_execution()`:
   - Builds `ContextResolver` from all blocks above the target block
   - Creates `ExecutionContext` with the resolver and document handle
3. **Clear active context** for the block (reset from previous runs)
4. **Get the block** from the document
5. **Call `block.execute(context)`** which returns an `ExecutionHandle`
6. **Store the handle** in `state.block_executions` for cancellation

### 5. Async Execution

Blocks spawn background tasks for actual execution:

```rust
// Example from Script block:
tokio::spawn(async move {
    // Emit BlockStarted event
    // Run the script
    let (exit_code, captured_output) = self.run_script(context, cancellation_token).await;

    // Store output variable in active context
    if let Some(var_name) = &self.output_variable {
        document_handle.update_active_context(block_id, |ctx| {
            ctx.insert(DocumentVar(var_name.clone(), output.clone()));
        }).await;
    }

    // Update execution status and emit BlockFinished event
    context.block_finished(exit_code, true);

});
```

### 6. Context Updates During Execution

Blocks can update their context during execution:

- `context.update_active_context()` - Store execution results (output variables, execution output)
- `context.update_passive_context()` - Update passive context (rare, for blocks that change based on execution)

### 7. Handle Storage

The execution handle is stored in global state for cancellation:

```rust
// In AtuinState:
pub block_executions: Arc<RwLock<HashMap<Uuid, ExecutionHandle>>>,

// After execution starts:
state.block_executions.write().await.insert(execution_id, handle);
```

## Context Management

### Passive vs Active Context

Each block has two independent contexts:

1. **Passive Context** - Set automatically when document changes
   - Evaluated by calling `block.passive_context(resolver, block_local_value_provider)`
   - Provides context for blocks below it (e.g., Directory sets cwd, Var sets variables)
   - Rebuilt when document structure changes or when a block's local value changes
   - Used to build `ContextResolver` for execution
   - **Not persisted to disk** - always computed from the document structure

2. **Active Context** - Set during block execution
   - Stores execution results (output variables, execution output via `BlockExecutionOutput`)
   - Can modify context for blocks below (but only after execution completes)
   - Cleared before each execution (when `execute_block` is called)
   - **Persisted to disk** via `ContextStorage` - survives app restarts
   - Loaded from disk when a document is opened
   - Can be manually cleared via the "Clear all active context" button in the UI

**Key Difference:** Passive context represents what the block _declares_ it will do (based on its configuration), while active context represents what the block _actually did_ (the results of execution). Both contribute to the `ContextResolver` for blocks below them, allowing both declarative context (passive) and execution results (active) to flow down the document.

### How Passive Contexts Work

Context blocks implement `passive_context()` to provide their context:

```rust
// Directory block
impl BlockBehavior for Directory {
    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn BlockLocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_path = resolver.resolve_template(&self.path)?;
        context.insert(DocumentCwd(resolved_path));
        Ok(Some(context))
    }
}

// Environment block
impl BlockBehavior for Environment {
    async fn passive_context(&self, resolver: &ContextResolver, _: Option<&dyn BlockLocalValueProvider>)
        -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_name = resolver.resolve_template(&self.name)?;
        let resolved_value = resolver.resolve_template(&self.value)?;
        context.insert(DocumentEnvVar(resolved_name, resolved_value));
        Ok(Some(context))
    }
}

// Var block
impl BlockBehavior for Var {
    async fn passive_context(&self, resolver: &ContextResolver, _: Option<&dyn BlockLocalValueProvider>)
        -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_name = resolver.resolve_template(&self.name)?;
        let resolved_value = resolver.resolve_template(&self.value)?;
        context.insert(DocumentVar(resolved_name, resolved_value));
        Ok(Some(context))
    }
}
```

### Context Building Flow

**On Document Open:**
1. **Parse blocks** from document JSON
2. **Load active contexts** from disk for each block (if available)
3. **Build passive contexts** for all blocks (see flow below)
4. **Create `BlockWithContext`** instances with both passive and active contexts
5. **Send initial context** to frontend

**On Document Update:**
1. **Document receives update** (new blocks, changed blocks, etc.)
2. **Identifies affected range** - finds earliest block that needs context rebuild
3. **Rebuilds passive contexts** starting from that index:
   - For each block, builds `ContextResolver` from all blocks above it (including both passive and active contexts)
   - Calls `block.passive_context(resolver, block_local_value_provider)` to get the block's new passive context
   - Updates the block's passive context (active context remains unchanged)
   - Adds the block's contexts to the resolver for the next block
4. **Sends context updates** to frontend via document bridge

**On Block Execution:**
1. **Active context is cleared** for the block being executed
2. **Block executes**, potentially calling `context.update_active_context()` to store results
3. **Active context is persisted** to disk after execution completes
4. **Passive contexts are rebuilt** since the changed active context may affect them

### Context Inheritance & Resolution

- **Sequential processing**: Passive contexts are built in document order
- **Cumulative effects**: Each block sees the context from all blocks above it
- **Snapshot at execution**: Execution context is a snapshot of passive contexts + previous active contexts
- **Template resolution**: `ContextResolver::resolve_template()` resolves minijinja template syntax (e.g., `{{ var.name }}` for variables)

### Variable Storage

Output variables are stored in the block's active context:

```rust
// During execution, store output variable:
context.update_active_context(block_id, |ctx| {
    ctx.insert(DocumentVar(var_name.clone(), output.clone()));
}).await;
```

Variables from active contexts are:
- **Included in the `ContextResolver`** for blocks below, so they can be used in templates
- **Persisted to disk** via `ContextStorage` when the execution completes
- **Loaded from disk** when the document is opened, allowing variables to survive app restarts
- **Cleared** when the block is re-executed or when the user clears all active context

This enables workflows where expensive operations (like querying a database or running a long computation) only need to be run once, with the results persisting across sessions.

## Cancellation & Error Handling

### Cancellation Mechanism

The system uses `CancellationToken` for graceful shutdown:

```rust
pub struct CancellationToken {
    sender: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
    receiver: Arc<std::sync::Mutex<Option<oneshot::Receiver<()>>>>,
}
```

**Cancellation Flow:**
1. **Frontend calls** `cancel_block_execution` with execution ID
2. **Backend looks up** the ExecutionHandle in global state
3. **Calls** `handle.cancellation_token.cancel()`
4. **Running task** receives cancellation signal via `tokio::select!`
5. **Task cleans up** and sets status to `Cancelled`

```rust
// Example cancellation handling in ScriptHandler:
tokio::select! {
    _ = cancel_rx => {
        // Kill the process gracefully
        if let Some(pid) = pid {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        }
        return (Err("Script execution cancelled".into()), captured);
    }
    result = child.wait() => {
        // Normal completion
    }
}
```

### Error Handling

**Error Types:**
- **Spawn errors**: Failed to start process/connection
- **Execution errors**: Process failed, network timeout, etc.
- **Cancellation**: User-requested stop
- **System errors**: Out of memory, permission denied, etc.

**Error Propagation:**
1. **Handler level**: Errors are captured and converted to `ExecutionStatus::Failed`
2. **Status updates**: Error messages are stored in the ExecutionHandle
3. **Frontend notification**: Errors are sent via output channels and lifecycle events

## Developer Guide

### Adding New Execution Blocks

1. **Define the block struct** in `backend/src/runtime/blocks/my_block.rs`:
```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;
use crate::runtime::blocks::{Block, BlockBehavior, FromDocument};
use crate::runtime::blocks::handler::{ExecutionContext, ExecutionHandle};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct MyBlock {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    // ... other fields
}

impl FromDocument for MyBlock {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data.get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let props = block_data.get("props")
            .and_then(|p| p.as_object())
            .ok_or("Invalid or missing props")?;

        // Extract fields from props
        Ok(MyBlock::builder()
            .id(id)
            .name(props.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string())
            .build())
    }
}

#[async_trait]
impl BlockBehavior for MyBlock {
    fn into_block(self) -> Block {
        Block::MyBlock(self)
    }

    fn id(&self) -> Uuid {
        self.id
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let handle = context.handle();

        // Spawn background task
        tokio::spawn(async move {
            // Mark block as started - sends events to Grand Central, workflow bus, and frontend
            let _ = context.block_started().await;

            // Do the actual work
            // Use context.context_resolver to access cwd, env vars, variables, etc.
            let cwd = context.context_resolver.cwd();
            let vars = context.context_resolver.vars();

            // Example: Store result in active context
            let result = "some output";
            let _ = context.update_active_context(context.block_id, |ctx| {
                ctx.insert(BlockExecutionOutput {
                    exit_code: Some(0),
                    stdout: Some(result.to_string()),
                    stderr: None,
                });
            }).await;

            // Mark block as finished - sends appropriate events
            let _ = context.block_finished(Some(0), true).await;
        });

        Ok(Some(handle))
    }
}
```

2. **Add to Block enum** in `backend/src/runtime/blocks/mod.rs`:
```rust
pub enum Block {
    // ... existing variants
    MyBlock(my_block::MyBlock),
}
```

3. **Add to Block methods** in `backend/src/runtime/blocks/mod.rs`:
```rust
impl Block {
    pub fn id(&self) -> Uuid {
        match self {
            // ... existing cases
            Block::MyBlock(my_block) => my_block.id,
        }
    }

    pub fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        match block_type {
            // ... existing cases
            "my-block" => Ok(Block::MyBlock(MyBlock::from_document(block_data)?)),
            // ...
        }
    }

    pub async fn passive_context(&self, ...) -> Result<Option<BlockContext>, _> {
        match self {
            // ... existing cases
            Block::MyBlock(my_block) => my_block.passive_context(resolver, block_local_value_provider).await,
        }
    }

    pub async fn execute(self, context: ExecutionContext) -> Result<Option<ExecutionHandle>, _> {
        match self {
            // ... existing cases
            Block::MyBlock(my_block) => my_block.execute(context).await,
        }
    }
}
```

4. **Add module to** `backend/src/runtime/blocks/mod.rs`:
```rust
pub(crate) mod my_block;
```

### Adding New Context Blocks

Follow the pattern in `backend/src/runtime/blocks/directory.rs` or `environment.rs`:

1. **Create module** `backend/src/runtime/blocks/my_context.rs`
2. **Define block struct** with `TypedBuilder`, `Serialize`, `Deserialize`, implementing `FromDocument` and `BlockBehavior`
3. **Define a context type** (e.g., `pub struct MyContextValue(pub String);`) to store in `BlockContext`
4. **Implement `typetag::serde`** for your context type if it needs to be serialized (for active context persistence)
5. **Implement `passive_context()`** to return your context value
6. **Update `ContextResolver`** in `backend/src/runtime/blocks/document/block_context.rs` to accumulate your context type
7. **Add comprehensive tests** covering edge cases

Example:
```rust
use crate::runtime::blocks::{Block, BlockBehavior, FromDocument};
use crate::runtime::blocks::document::block_context::{BlockContext, ContextResolver};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
pub struct MyContext {
    pub id: Uuid,
    pub value: String,
}

// Define a type to store in BlockContext
pub struct MyContextValue(pub String);

impl FromDocument for MyContext {
    // ... parse from JSON
}

#[async_trait]
impl BlockBehavior for MyContext {
    fn into_block(self) -> Block {
        Block::MyContext(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _: Option<&dyn BlockLocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_value = resolver.resolve_template(&self.value)?;
        context.insert(MyContextValue(resolved_value));
        Ok(Some(context))
    }
}
```

Then update `ContextResolver::push_block()` to accumulate values of type `MyContextValue`.

### Testing Patterns

**Context Block Tests:**
- Basic functionality
- Edge cases (empty values, special characters)
- Error handling (validation failures)
- Serialization (JSON round-trip)
- Integration (multiple contexts, field preservation)

**Execution Block Tests:**
- Successful execution
- Error scenarios
- Cancellation handling
- Output capture
- Variable substitution

## Examples & Patterns

### Simple Script Execution

```rust
// 1. Document contains these blocks:
// Block 0: Directory { path: "/tmp" }
// Block 1: Environment { name: "DEBUG", value: "1" }
// Block 2: Script { code: "echo $DEBUG" }

// 2. When Script executes, ExecutionContext is built with:
// - context_resolver built from blocks 0-1
// - context_resolver.cwd() = "/tmp"
// - context_resolver.env_vars() = {"DEBUG": "1"}

// 3. Script uses context_resolver to:
// - Get working directory: context.context_resolver.cwd()
// - Get env vars: context.context_resolver.env_vars()
// - Resolve templates: context.context_resolver.resolve_template("echo {{ env.DEBUG }}")
```

### Complex Workflow

```rust
// Runbook blocks in order:
// 1. Directory { path: "/app" }
// 2. Environment { name: "NODE_ENV", value: "production" }
// 3. Host { host: "deploy@server.com" }
// 4. Script { code: "npm run build", output_variable: "build_result" }
// 5. Script { code: "echo 'Build: {{ var.build_result }}'" }

// Passive context building (when document loads/changes):
// Block 1: passive_context() inserts DocumentCwd("/app")
// Block 2: passive_context() inserts DocumentEnvVar("NODE_ENV", "production")
// Block 3: passive_context() inserts DocumentSshHost("deploy@server.com")
// Block 4: passive_context() returns None (execution block)
// Block 5: passive_context() returns None (execution block)

// When block 4 executes:
// - ContextResolver built from blocks 0-3 (passive contexts)
// - resolver.cwd() = "/app"
// - resolver.env_vars() = {"NODE_ENV": "production"}
// - resolver.ssh_host() = Some("deploy@server.com")
// - Script runs via SSH: ssh deploy@server.com "cd /app && NODE_ENV=production npm run build"
// - On success, block 4's active context updated with DocumentVar("build_result", output)

// When block 5 executes:
// - ContextResolver built from blocks 0-4 (passive + active contexts)
// - resolver.vars() = {"build_result": "<output from block 4>"}
// - Script resolves template: "echo 'Build: {{ var.build_result }}'" → "echo 'Build: <output from block 4>'"
```

### Cancellation Example

```rust
// 1. Start long-running script
let document = state.documents.get(&runbook_id)?;
let context = document.start_execution(block_id, ...).await?;
let block = document.get_block(block_id).await?;
let handle = block.execute(context).await?;

// 2. Store handle for later cancellation
state.block_executions.insert(handle.id, handle);

// 3. User clicks cancel button
// 4. Frontend calls cancel_block_execution(handle.id)
// 5. Backend finds handle and calls cancel()
handle.cancellation_token.cancel();

// 6. Running script receives signal via tokio::select! and cleans up
// 7. Status updated to ExecutionStatus::Cancelled
```

## Architecture Benefits

### Type Safety
- **Compile-time guarantees** for block structure
- **Type-safe context storage** using `TypeId` (no stringly-typed context)
- **Clear interfaces** between components via `BlockBehavior` trait
- **Strongly-typed context values** (DocumentCwd, DocumentVar, etc.)

### Performance
- **Direct method dispatch** via enum matching (no trait objects for hot path)
- **Async execution** doesn't block the main thread
- **Efficient cancellation** via tokio channels
- **Actor-based document management** prevents lock contention
- **Incremental context rebuilding** - only rebuilds affected blocks when document changes

### Maintainability
- **Clear separation** of data (Block structs) and behavior (BlockBehavior implementations)
- **Modular design** - each block is self-contained in its own module
- **Comprehensive testing** ensures reliability
- **Self-documenting** code with clear patterns
- **Passive/Active context separation** makes it clear when contexts are updated

### Extensibility
- **Easy to add new block types** - just implement BlockBehavior
- **Flexible context system** - add new context types by defining new structs
- **Event system** for monitoring and debugging (Grand Central)
- **Variable system** enables block chaining and workflows
- **Document bridge** allows real-time updates to frontend
- **Actor pattern** allows for future improvements (e.g., persistent documents, undo/redo)

This execution system provides a robust foundation for running complex, multi-step runbooks with proper error handling, cancellation, and state management.
