# Atuin Desktop Backend Execution System

This document provides a comprehensive overview of how the Atuin Desktop backend executes runbook blocks, manages execution context, and handles cancellation.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Core Components](#core-components)
- [Execution Flow](#execution-flow)
- [Context Management](#context-management)
- [Serial Execution](#serial-execution)
- [Cancellation & Error Handling](#cancellation--error-handling)

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

### Block State

Some blocks can maintain their own internal state that exists independently from context. Block state is:
- **Defined per block type**: Each block can optionally define a state struct implementing the `BlockState` trait
- **Serializable**: State is sent to the frontend via `BlockStateChanged` messages
- **Persistent during runtime**: State survives document updates and context rebuilds
- **Accessible on frontend**: Blocks can use `useBlockState(block_id)` to access and react to state changes

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

    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        None
    }
}
```

**Key Methods:**
- `into_block()` - Converts the block into the generic `Block` enum
- `id()` - Returns the unique identifier (UUID) for this block instance
- `passive_context()` - Returns the passive context this block provides (evaluated on document changes)
- `execute()` - Performs the actual block execution (returns immediately with a handle)
- `create_state()` - Optionally returns initial state for the block (called when block is created)

### BlockState Trait

The `BlockState` trait allows blocks to maintain internal state that can be updated and communicated to the frontend:

```rust
pub trait BlockState: erased_serde::Serialize + Send + Sync + std::fmt::Debug + Any {}
```

**Purpose:**
- **Runtime state management**: Store block-specific state that doesn't belong in context
- **Frontend communication**: State changes are automatically serialized and sent to frontend via `BlockStateChanged` messages
- **Type-safe access**: Uses the `BlockStateExt` trait to downcast to concrete state types

**Example - Dropdown Block State:**
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct DropdownBlockState {
    pub selected_option: Option<String>,
}

impl BlockState for DropdownBlockState {}

// In the Dropdown block implementation:
impl BlockBehavior for Dropdown {
    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(DropdownBlockState {
            selected_option: None,
        }))
    }
}
```

**Accessing and Updating State:**

Blocks can access and modify their state through `ExecutionContext`:

```rust
execution_context.update_block_state::<DropdownState, _>(self.id, |state| {
    state.resolved = ...;
}).await?;
```

### ExecutionContext

The `ExecutionContext` contains all the information needed to execute a block:

```rust
pub struct ExecutionContext {
    pub(crate) block_id: Uuid,                       // ID of the block being executed
    pub(crate) runbook_id: Uuid,                     // Which runbook this execution belongs to
    document_handle: Arc<DocumentHandle>,            // Handle to the document actor (private - use helper methods)
    pub(crate) context_resolver: Arc<ContextResolver>, // Resolves templates and provides context values
    output_channel: Option<Arc<dyn MessageChannel<DocumentBridgeMessage>>>, // For sending output to frontend
    workflow_event_sender: broadcast::Sender<WorkflowEvent>, // For sending workflow events
    pub(crate) ssh_pool: Option<SshPoolHandle>,      // SSH connection pool (if available)
    pub(crate) pty_store: Option<PtyStoreHandle>,    // PTY store for terminal blocks (if available)
    pub(crate) gc_event_bus: Option<Arc<dyn EventBus>>, // Grand Central event bus
    handle: ExecutionHandle,                         // Handle for this execution
}
```

**Key Fields:**
- `block_id` - The UUID of the block currently being executed
- `context_resolver` - Wrapped in Arc, provides access to variables, cwd, env vars, ssh_host via `resolve_template()` and getter methods
- `document_handle` - (Private) Handle to the document actor; use helper methods (`update_active_context`, etc.) instead of direct access
- `handle` - The execution handle for this block execution

**Helper Methods:**
The ExecutionContext provides several convenience methods to simplify block implementations:

- **Lifecycle management:**
  - `block_started()` - Marks block as started, sets status to Running, and sends events to Grand Central, workflow bus, and frontend
  - `block_finished(exit_code, success)` - Marks block as finished, sets status to Success, and sends finish events
  - `block_failed(error)` - Marks block as failed with an error message, sets status to Failed, and sends error events
  - `block_cancelled()` - Marks block as cancelled, sets status to Cancelled, and sends cancellation events

- **Context management:**
  - `update_passive_context<F>(block_id, update_fn)` - Update a block's passive context with a closure
  - `update_active_context<F>(block_id, update_fn)` - Update a block's active context with a closure
  - `clear_active_context(block_id)` - Clear a block's active context (resets to empty BlockContext)

- **State management:**
  - `update_block_state<T, F>(block_id, update_fn)` - Update a block's state with type-safe access to the state struct

- **Communication:**
  - `send_output(message)` - Send output (stdout, stderr, lifecycle events) to the frontend via the document bridge
  - `emit_workflow_event(event)` - Emit a workflow event to the workflow event bus
  - `emit_gc_event(event)` - Emit a Grand Central event for monitoring and serial execution tracking
  - `prompt_client(prompt)` - Prompt the client for input (e.g., password, confirmation) and await the response

- **Cancellation:**
  - `cancellation_token()` - Get the cancellation token for this execution
  - `cancellation_receiver()` - Get a oneshot receiver for cancellation signals (consumed by tokio::select!)

- **Accessors:**
  - `handle()` - Get the ExecutionHandle for this execution
  - `context_resolver` - Access the ContextResolver for template resolution and context values

**Lifecycle:**
1. **Created** by `Document::build_execution_context()` when a block execution is requested
2. **Contains** a snapshot of the cumulative context from all blocks above the executing block
3. **Used** by blocks to resolve templates and access context values
4. **Discarded** after execution completes

### ExecutionHandle

The `ExecutionHandle` represents a running or completed block execution:

```rust
pub struct ExecutionHandle {
    pub id: Uuid,                                    // Unique execution ID
    pub block_id: Uuid,                              // ID of the block being executed
    pub cancellation_token: CancellationToken,       // For graceful cancellation
    pub status: Arc<RwLock<ExecutionStatus>>,        // Current execution status
    pub prompt_callbacks: Arc<Mutex<HashMap<Uuid, oneshot::Sender<ClientPromptResult>>>>, // Callbacks for client prompts
}
```

**Purpose:**
- **Async Management**: Allows tracking of long-running operations
- **Cancellation**: Provides mechanism to stop execution gracefully
- **Status Monitoring**: Frontend can poll execution status
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


**BlockContextItem**

To store a value in context, implement the `BlockContextItem` trait and use `typetag::serde` for serialization.

**Example Implementation:**

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVar {
    // ...
}

#[typetag::serde]
impl BlockContextItem for DocumentVar {}
```

**BlockWithContext**
Wrapper that pairs a block with its contexts and state:

```rust
pub struct BlockWithContext {
    block: Block,
    passive_context: BlockContext,
    active_context: BlockContext,
    state: Option<Box<dyn BlockState>>,
}
```

When constructing a `BlockWithContext`, you can optionally provide an active context (e.g., loaded from disk) and state. If not provided, empty context and no state are used.

**Key differences between context and state:**
- **Context** (passive/active): Affects execution environment for blocks below it; can be persisted to disk
- **State**: Block-specific runtime data that doesn't affect other blocks; communicated to frontend for UI updates

**ContextResolver**
Resolves templates and provides access to cumulative context from blocks above:

```rust
pub struct ContextResolver {
    vars: HashMap<String, String>,                 // Variables from Var blocks
    cwd: String,                                   // Working directory from Directory blocks
    env_vars: HashMap<String, String>,             // Environment variables from Environment blocks
    ssh_host: Option<String>,                      // SSH host from Host/SshConnect blocks
    extra_template_context: HashMap<String, Value> // Extra template context for blocks
}
```

**ResolvedContext**
A serializable snapshot of context for the frontend:

```rust
pub struct ResolvedContext {
    pub variables: HashMap<String, String>,
    pub variables_sources: HashMap<String, String>,
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
5. Creates initial state for blocks that define `create_state()`
6. Builds initial passive contexts for all blocks

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
3. **Create initial state** for blocks via `block.create_state()` (if defined)
4. **Build passive contexts** for all blocks (see flow below)
5. **Create `BlockWithContext`** instances with both passive and active contexts, and state
6. **Send initial context** to frontend
7. **Send initial state** to frontend via `BlockStateChanged` messages (for blocks with state)

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

### Block State Updates

Block state is separate from context and is used for runtime data that needs to be communicated to the frontend but doesn't affect other blocks:

**State Update Flow:**
1. **Block modifies its state** by accessing `block_with_context.state_mut()`
2. **Document emits state change** via `document.emit_state_changed(block_id, state)`
3. **State is serialized** to JSON using `erased_serde::Serialize`
4. **Frontend receives** `BlockStateChanged` message via document bridge
5. **Frontend components** using `useBlockState(block_id)` react to the change

**Example - Updating Dropdown Selection:**
```rust
// In the document actor, when handling a state update command:
if let Some(block) = self.document.get_block_mut(&block_id) {
    if let Some(state) = block.state_mut() {
        if let Some(dropdown_state) = state.downcast_mut::<DropdownBlockState>() {
            dropdown_state.selected_option = Some("option1".to_string());
            // Emit state changed event to frontend
            self.document.emit_state_changed(block_id, state).await?;
        }
    }
}
```

**State vs Context:**
- **State**: Block-specific data, temporary runtime values, etc; sent to frontend on changes
- **Context**: Execution environment (variables, cwd, env vars); affects blocks below; can be persisted
- State is NOT persisted to disk and is NOT included in context resolution for other blocks

## Serial Execution

Serial execution allows running all blocks in a runbook sequentially, from top to bottom, stopping if any block fails or is cancelled. This feature was migrated from a frontend-based implementation to a backend command system for better reliability and monitoring.

### Architecture

**Backend Command:**
The `start_serial_execution` Tauri command manages the entire serial execution lifecycle:

```rust
#[tauri::command]
pub async fn start_serial_execution(
    app: AppHandle,
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String>
```

**Key Components:**

1. **Execution Handle Tracking** - Each block execution returns an `ExecutionHandle` with a `finished_channel()` that emits `ExecutionResult` when complete
2. **Sequential Coordination** - Backend spawns a task that executes blocks one at a time, waiting for each to finish before starting the next
3. **Cancellation Support** - Serial execution can be cancelled via `cancel_serial_execution` command, which cancels the current block and stops the sequence
4. **Grand Central Events** - Serial execution lifecycle is tracked via GC events for UI updates

### Execution Flow

1. **Frontend initiates** serial execution via `start_serial_execution(document_id)`
2. **Backend retrieves** all block IDs from the document in order
3. **Backend spawns** background task that:
   - Iterates through blocks sequentially
   - For each block:
     - Calls `execute_single_block()` to start execution
     - Stores the `ExecutionHandle` in global state
     - Subscribes to the handle's `finished_channel()`
     - Uses `tokio::select!` to wait for either:
       - Block completion (via `finished_channel`)
       - Cancellation signal (via cancellation receiver)
   - If block succeeds (`ExecutionResult::Success`), continues to next block
   - If block fails (`ExecutionResult::Failure`) or is cancelled (`ExecutionResult::Cancelled`), stops sequence
4. **Backend emits** Grand Central events at key points:
   - `SerialExecutionStarted` - When serial execution begins
   - `SerialExecutionCompleted` - When all blocks complete successfully
   - `SerialExecutionFailed` - When a block fails
   - `SerialExecutionCancelled` - When execution is cancelled

### ExecutionResult Enum

```rust
pub enum ExecutionResult {
    Success,   // Block completed successfully (exit code 0)
    Failure,   // Block failed (non-zero exit code or execution error)
    Cancelled, // Block was cancelled by user
}
```

This enum is sent through the `finished_channel` when a block execution completes, allowing serial execution to determine whether to continue or stop.

### ExecutionHandle Lifecycle Events

The `ExecutionHandle` now includes a `finished_channel` that provides a watch channel for monitoring completion:

```rust
pub struct ExecutionHandle {
    // ... other fields
    pub on_finish: (
        watch::Sender<Option<ExecutionResult>>,
        watch::Receiver<Option<ExecutionResult>>,
    ),
}

impl ExecutionHandle {
    pub fn finished_channel(&self) -> watch::Receiver<Option<ExecutionResult>> {
        self.on_finish.1.clone()
    }
}
```

**Helper Methods in ExecutionContext:**
- `block_finished()` - Sends `ExecutionResult::Success` through the channel
- `block_failed()` - Sends `ExecutionResult::Failure` through the channel
- `block_cancelled()` - Sends `ExecutionResult::Cancelled` through the channel

### Grand Central Serial Execution Events

```rust
pub enum GCEvent {
    SerialExecutionStarted { runbook_id: Uuid },
    SerialExecutionCompleted { runbook_id: Uuid },
    SerialExecutionCancelled { runbook_id: Uuid },
    SerialExecutionFailed { runbook_id: Uuid, error: String },
    // ... other events
}
```

**Frontend Integration:**
The frontend subscribes to these events to update UI state (e.g., showing/hiding a progress indicator, updating the play button state):

```typescript
useEffect(() => {
  const unsubscribe = subscribeToGCEvents((event) => {
    if (event.type === 'SerialExecutionStarted' && event.data.runbookId === currentRunbookId) {
      setIsSerialExecutionRunning(true);
    } else if (['SerialExecutionCompleted', 'SerialExecutionFailed', 'SerialExecutionCancelled']
      .includes(event.type) && event.data.runbookId === currentRunbookId) {
      setIsSerialExecutionRunning(false);
    }
  });
  return unsubscribe;
}, [currentRunbookId]);
```

### Cancellation

Serial execution can be cancelled at any time:

```rust
#[tauri::command]
pub async fn cancel_serial_execution(
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String>
```

**Cancellation Flow:**
1. Frontend calls `cancel_serial_execution(document_id)`
2. Backend looks up the cancellation sender for the serial execution
3. Backend sends cancellation signal through the oneshot channel
4. Serial execution task receives signal via `tokio::select!`
5. Current block execution is cancelled via `handle.cancellation_token.cancel()`
6. Serial execution loop breaks and emits `SerialExecutionCancelled` event
7. Cleanup removes handles from global state

### Benefits of Backend Implementation

**Previous (Frontend-based):**
- Complex coordination logic in TypeScript
- Race conditions when managing execution state
- Difficult to track execution across page refreshes
- Limited error handling and recovery

**Current (Backend-based):**
- Centralized execution logic in Rust
- Reliable state management via actor system
- Proper async/await coordination with tokio
- Comprehensive event emission for monitoring
- Consistent error handling via Result types
- Execution survives frontend navigation (within same session)

### State Management

**Global State:**
```rust
pub struct AtuinState {
    pub serial_executions: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    pub block_executions: Arc<RwLock<HashMap<Uuid, ExecutionHandle>>>,
    // ... other fields
}
```

- `serial_executions` - Maps document IDs to cancellation senders for active serial executions
- `block_executions` - Maps execution IDs to handles for all running block executions (both serial and individual)

### Example Usage

**Starting Serial Execution:**
```typescript
await invoke('start_serial_execution', { documentId: runbookId });
```

**Cancelling Serial Execution:**
```typescript
await invoke('cancel_serial_execution', { documentId: runbookId });
```

**Monitoring Progress:**
```typescript
const [runningBlocks, setRunningBlocks] = useState(0);

useEffect(() => {
  const unsubscribe = subscribeToGCEvents((event) => {
    if (event.data?.runbookId === runbookId) {
      if (event.type === 'BlockStarted') {
        setRunningBlocks(prev => prev + 1);
      } else if (['BlockFinished', 'BlockFailed', 'BlockCancelled'].includes(event.type)) {
        setRunningBlocks(prev => prev - 1);
      }
    }
  });
  return unsubscribe;
}, [runbookId]);
```

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

**Process Group Cancellation (Script Blocks):**

For script blocks, proper cancellation requires killing the entire process group, not just the parent shell process. This ensures that complex command pipelines (e.g., `seq 1 20 | xargs -I {} sh -c 'echo {}; sleep 1'`) are fully terminated.

```rust
// When spawning the process (Unix):
#[cfg(unix)]
{
    cmd.process_group(0); // Makes the child a process group leader
}

// When cancelling (Unix):
tokio::select! {
    _ = cancel_rx => {
        // Kill the entire process group
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;
            if let Some(pid) = pid {
                // Negative PID targets the process group
                let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
            }
        }
        #[cfg(windows)]
        {
            let _ = child.kill().await;
        }
        return (Err("Script execution cancelled".into()), captured);
    }
    result = child.wait() => {
        // Normal completion
    }
}
```

**Why Process Groups Matter:**
- **Simple scripts** (e.g., `echo "hello"; sleep 5`) run entirely within the shell process, so killing the shell suffices
- **Pipelines** (e.g., `cat file | grep pattern`) spawn multiple processes that form a pipeline
- **Command substitution** (e.g., `xargs`, `parallel`) spawn many child processes
- Without process group termination, child processes become orphaned and continue running after cancellation

### Grand Central Event System

The execution system includes a Grand Central event bus that provides global monitoring of block execution lifecycle events. These events enable features like serial execution tracking and UI badge notifications.

**GCEvent Types:**
```rust
pub enum GCEvent {
    BlockStarted { block_id: Uuid, runbook_id: Uuid },
    BlockFinished { block_id: Uuid, runbook_id: Uuid, success: bool },
    BlockFailed { block_id: Uuid, runbook_id: Uuid, error: String },
    BlockCancelled { block_id: Uuid, runbook_id: Uuid },
}
```

**Event Flow:**
1. **Block execution starts** - `context.block_started()` emits `BlockStarted` event
2. **Block completes** - One of the following events is emitted:
   - `BlockFinished` - Normal completion (success or failure based on exit code)
   - `BlockFailed` - Execution error or non-zero exit code
   - `BlockCancelled` - User-requested cancellation
3. **Frontend subscribes** to events via `subscribeToGCEvents()` and updates UI accordingly

**Use Cases:**
- **Tab badges** - Track running block count per runbook tab
- **Serial execution** - Monitor when all blocks in a sequence complete
- **Execution monitoring** - Dashboard showing all running blocks across all runbooks
- **Analytics** - Track block execution patterns and failures

**Example - Tab Badge Counter:**
```typescript
useEffect(() => {
  const unsubscribe = subscribeToGCEvents((event) => {
    if (event.runbookId === currentRunbookId) {
      if (event.type === 'BlockStarted') {
        incrementBadge(1);
      } else if (['BlockFinished', 'BlockFailed', 'BlockCancelled'].includes(event.type)) {
        decrementBadge(1);
      }
    }
  });
  return unsubscribe;
}, [currentRunbookId]);
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
3. **Frontend notification**: Errors are sent via output channels, lifecycle events, and Grand Central events
