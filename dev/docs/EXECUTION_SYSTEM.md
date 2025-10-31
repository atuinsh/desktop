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
- `passive_context()` - For context blocks, returns the context this block provides (evaluated on document changes)
- `execute()` - For execution blocks, performs the actual block execution (returns immediately with a handle)

### ExecutionContext

The `ExecutionContext` contains all the information needed to execute a block:

```rust
pub struct ExecutionContext {
    pub runbook_id: Uuid,                     // Which runbook this execution belongs to
    pub document_handle: Arc<DocumentHandle>, // Handle to the document actor for context updates
    pub context_resolver: ContextResolver,    // Resolves templates and provides context values
    pub output_channel: Option<Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>>, // For sending output to frontend
    pub event_sender: broadcast::Sender<WorkflowEvent>, // For sending workflow events
    pub ssh_pool: Option<SshPoolHandle>,      // SSH connection pool (if available)
    pub pty_store: Option<PtyStoreHandle>,    // PTY store for terminal blocks (if available)
    pub event_bus: Option<Arc<dyn EventBus>>, // Grand Central event bus
}
```

**Key Fields:**
- `context_resolver` - Provides access to variables, cwd, env vars, ssh_host via `resolve_template()` and getter methods
- `document_handle` - Used to update block's active context (e.g., storing output variables)

**Lifecycle:**
1. **Created** by `Document::build_execution_context()` when a block execution is requested
2. **Contains** a snapshot of the cumulative context from all blocks above the executing block
3. **Used** by execution blocks to resolve templates and access context values
4. **Discarded** after execution completes

### ExecutionHandle

The `ExecutionHandle` represents a running or completed block execution:

```rust
pub struct ExecutionHandle {
    pub id: Uuid,                   // Unique execution ID
    pub block_id: Uuid,             // ID of the block being executed
    pub cancellation_token: CancellationToken, // For graceful cancellation
    pub status: Arc<RwLock<ExecutionStatus>>,   // Current execution status
    pub output_variable: Option<String>,        // Output variable name (if any)
}
```

**Purpose:**
- **Async Management**: Allows tracking of long-running operations
- **Cancellation**: Provides mechanism to stop execution gracefully
- **Status Monitoring**: Frontend can poll execution status
- **Output Capture**: Links execution results to variables for use in subsequent blocks

### ExecutionStatus

Tracks the current state of a block execution:

```rust
pub enum ExecutionStatus {
    Running,
    Success(String),    // Contains the output value
    Failed(String),     // Contains error message
    Cancelled,
}
```

### Context System Components

**BlockContext**
A type-safe storage for context values using `TypeId`:

```rust
pub struct BlockContext {
    entries: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
```

Each block has two contexts:
- **Passive context** - Evaluated when the document changes, provides context for blocks below it
- **Active context** - Set during execution, stores output variables and execution results

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
4. Builds initial passive contexts for all blocks

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

    // Update execution status
    *handle.status.write().await = status;

    // Emit BlockFinished event
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
   - Evaluated by calling `block.passive_context(resolver)`
   - Provides context for blocks below it (e.g., Directory sets cwd)
   - Rebuilt when document structure changes
   - Used to build `ContextResolver` for execution

2. **Active Context** - Set during block execution
   - Stores execution results (output variables, execution output)
   - Can modify context for blocks below (but only after execution)
   - Cleared before each execution

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

1. **Document receives update** (new blocks, changed blocks, etc.)
2. **Identifies affected range** - finds earliest block that needs context rebuild
3. **Rebuilds contexts** starting from that index:
   - For each block, builds `ContextResolver` from all blocks above it
   - Calls `block.passive_context(resolver)` to get the block's context
   - Stores the context with the block as `BlockWithContext`
   - Adds the block's context to the resolver for the next block
4. **Sends context updates** to frontend via document bridge

### Context Inheritance & Resolution

- **Sequential processing**: Passive contexts are built in document order
- **Cumulative effects**: Each block sees the context from all blocks above it
- **Snapshot at execution**: Execution context is a snapshot of passive contexts + previous active contexts
- **Template resolution**: `ContextResolver::resolve_template()` resolves minijinja template syntax (e.g., `{{ var.name }}` for variables, `{{ env.NAME }}` for environment variables)

### Variable Storage

Output variables are stored in the block's active context:

```rust
// During execution, store output variable:
document_handle.update_active_context(block_id, |ctx| {
    ctx.insert(DocumentVar(var_name.clone(), output.clone()));
}).await;
```

Variables from active contexts are included in the `ContextResolver` for blocks below, so they can be used in templates.

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

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        // Create execution handle
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: self.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        let handle_clone = handle.clone();

        // Spawn background task
        tokio::spawn(async move {
            // Do the actual work
            // Use context.context_resolver to access cwd, env vars, variables, etc.
            // Use context.document_handle.update_active_context() to store results

            // Update handle status when complete
            *handle_clone.status.write().await = ExecutionStatus::Success("done".to_string());
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
4. **Implement `passive_context()`** to return your context value
5. **Update `ContextResolver`** in `backend/src/runtime/blocks/document/block_context.rs` to accumulate your context type
6. **Add comprehensive tests** covering edge cases

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
