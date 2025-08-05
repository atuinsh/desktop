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
Blocks that modify the execution environment for subsequent blocks:
- `Directory` - Change working directory
- `Environment` - Set environment variables
- `SshConnect` - Configure SSH connection

## Core Components

### BlockHandler Trait

The `BlockHandler` trait defines how blocks are executed:

```rust
#[async_trait]
pub trait BlockHandler: Send + Sync {
    type Block: Send + Sync;

    fn block_type(&self) -> &'static str;
    fn output_variable(&self, block: &Self::Block) -> Option<String>;
    
    async fn execute(
        &self,
        block: Self::Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: AppHandle,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>>;

    async fn cancel(&self, handle: &ExecutionHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

**Key Methods:**
- `block_type()` - Returns the string identifier for this block type
- `output_variable()` - Extracts the output variable name if the block produces one
- `execute()` - Performs the actual block execution (returns immediately with a handle)
- `cancel()` - Cancels a running execution

### ExecutionContext

The `ExecutionContext` contains all the environmental information needed to execute a block:

```rust
pub struct ExecutionContext {
    pub runbook_id: Uuid,           // Which runbook this execution belongs to
    pub cwd: String,                // Current working directory
    pub env: HashMap<String, String>, // Environment variables
    pub variables: HashMap<String, String>, // Template variables for substitution
    pub ssh_host: Option<String>,   // SSH connection info (if any)
    pub document: Vec<serde_json::Value>, // Full runbook document for template resolution
}
```

**Lifecycle:**
1. **Created** by `ContextBuilder::build_context()` when a block execution is requested
2. **Modified** by context blocks (Directory, Environment, SshConnect) as they're processed
3. **Used** by execution blocks to determine how to run (working directory, env vars, etc.)
4. **Stored** temporarily during execution, then discarded

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

### ContextProvider Trait

For context blocks that modify the execution environment:

```rust
pub trait ContextProvider: Send + Sync {
    type Block: Send + Sync;

    fn block_type(&self) -> &'static str;
    
    fn apply_context(
        &self,
        block: &Self::Block,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

## Execution Flow

### 1. Request Initiation

When the frontend requests block execution via the `execute_block` Tauri command:

```rust
#[tauri::command]
pub async fn execute_block(
    state: State<'_, AtuinState>,
    app_handle: AppHandle,
    block_id: String,
    runbook_id: String,
    editor_document: Vec<serde_json::Value>,
    output_channel: Channel<BlockOutput>,
) -> Result<String, String>
```

### 2. Context Building

The `ContextBuilder::build_context()` function:

1. **Parses the runbook document** to find all blocks
2. **Identifies context blocks** that come before the target block
3. **Applies context blocks in order** to build the execution environment
4. **Returns the final ExecutionContext**

```rust
// Example context building flow:
let context = ContextBuilder::build_context(&block_id, &editor_document, &runbook_id).await?;

// This processes blocks like:
// 1. Directory { path: "/tmp" }        -> context.cwd = "/tmp"
// 2. Environment { name: "DEBUG", value: "1" } -> context.env["DEBUG"] = "1"
// 3. SshConnect { user_host: "user@host" }     -> context.ssh_host = Some("user@host")
// 4. Script { code: "echo $DEBUG" }    -> Execute with the built context
```

### 3. Block Execution

The `BlockRegistry::execute_block()` method:

1. **Matches the block type** to the appropriate handler
2. **Calls the handler's execute method** with the context
3. **Returns an ExecutionHandle** immediately (execution happens in background)

```rust
// Registry uses direct dispatch (no type erasure):
match block {
    Block::Script(script) => {
        ScriptHandler.execute(script.clone(), context, event_sender, output_channel, app_handle).await
    }
    Block::Terminal(terminal) => {
        // TerminalHandler.execute(...)
    }
    // ... other block types
}
```

### 4. Async Execution

Block handlers spawn background tasks for actual execution:

```rust
// Example from ScriptHandler:
tokio::spawn(async move {
    let (exit_code, captured_output) = Self::run_script(
        &script_clone,
        context_clone,
        handle_clone.cancellation_token.clone(),
        event_sender_clone,
        output_channel_clone,
    ).await;

    // Update status based on result
    let status = match exit_code {
        Ok(0) => ExecutionStatus::Success(captured_output.trim().to_string()),
        Ok(code) => ExecutionStatus::Failed(format!("Process exited with code {}", code)),
        Err(e) => ExecutionStatus::Failed(e.to_string()),
    };

    *handle_clone.status.write().await = status;
});
```

### 5. Handle Storage

The execution handle is stored in global state for later access:

```rust
// In AtuinState:
pub block_executions: Arc<RwLock<HashMap<Uuid, ExecutionHandle>>>,

// After execution starts:
state.block_executions.write().await.insert(execution_id, handle.clone());
```

## Context Management

### Where Contexts Are Created

1. **Per-block execution**: Each `execute_block` call creates a fresh context
2. **Built from document**: Context is derived from the runbook document state
3. **Not persisted**: Contexts are temporary and discarded after execution

### How Contexts Are Modified

Context blocks modify the execution environment in sequence:

```rust
// Directory block
impl ContextProvider for DirectoryHandler {
    fn apply_context(&self, block: &Directory, context: &mut ExecutionContext) -> Result<(), _> {
        context.cwd = block.path.clone();  // Change working directory
        Ok(())
    }
}

// Environment block  
impl ContextProvider for EnvironmentHandler {
    fn apply_context(&self, block: &Environment, context: &mut ExecutionContext) -> Result<(), _> {
        context.env.insert(block.name.clone(), block.value.clone());  // Add env var
        Ok(())
    }
}

// SSH block
impl ContextProvider for SshConnectHandler {
    fn apply_context(&self, block: &SshConnect, context: &mut ExecutionContext) -> Result<(), _> {
        context.ssh_host = Some(block.user_host.clone());  // Configure SSH
        Ok(())
    }
}
```

### Context Inheritance

- **Sequential processing**: Context blocks are applied in document order
- **Cumulative effects**: Each context block builds on the previous state
- **Block isolation**: Each execution gets its own context copy

### Variable Storage

Output variables from successful executions are stored globally:

```rust
// In AtuinState:
pub runbook_output_variables: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
//                                                 ^^^^^^^^  ^^^^^^^^  ^^^^^^^^
//                                                 runbook   variable  value
//                                                 ID        name

// After successful execution with output variable:
state.runbook_output_variables
    .write().await
    .entry(runbook_id)
    .or_insert_with(HashMap::new)
    .insert(var_name.clone(), output.clone());
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

1. **Define the block struct** in its own module:
```rust
#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
pub struct MyBlock {
    pub id: Uuid,
    pub my_field: String,
    // ... other fields
}
```

2. **Create a handler**:
```rust
pub struct MyBlockHandler;

#[async_trait]
impl BlockHandler for MyBlockHandler {
    type Block = MyBlock;

    fn block_type(&self) -> &'static str { "my-block" }
    
    fn output_variable(&self, block: &Self::Block) -> Option<String> {
        block.output_variable.clone()
    }

    async fn execute(&self, block: MyBlock, context: ExecutionContext, ...) -> Result<ExecutionHandle, _> {
        // Create execution handle
        let handle = ExecutionHandle { /* ... */ };
        
        // Spawn background task
        tokio::spawn(async move {
            // Do the actual work
            // Update handle.status when complete
        });
        
        Ok(handle)
    }
}
```

3. **Add to Block enum**:
```rust
pub enum Block {
    // ... existing variants
    MyBlock(MyBlock),
}
```

4. **Add to registry**:
```rust
impl BlockRegistry {
    pub async fn execute_block(&self, block: &Block, ...) -> Result<ExecutionHandle, _> {
        match block {
            // ... existing cases
            Block::MyBlock(my_block) => {
                MyBlockHandler.execute(my_block.clone(), context, ...).await
            }
        }
    }
}
```

### Adding New Context Blocks

Follow the pattern in `backend/src/runtime/blocks/context/`:

1. **Create module** `context/my_context/mod.rs`
2. **Define block struct** with `TypedBuilder`, `Serialize`, `Deserialize`
3. **Create handler** implementing `ContextProvider`
4. **Add comprehensive tests** covering edge cases
5. **Export in** `context/mod.rs`

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
// 1. Context building finds these blocks before the script:
// Directory { path: "/tmp" }
// Environment { name: "DEBUG", value: "1" }

// 2. Context is built:
let context = ExecutionContext {
    cwd: "/tmp",
    env: {"DEBUG": "1"},
    // ... other fields
};

// 3. Script executes with this context:
// - Working directory: /tmp
// - Environment: DEBUG=1
// - Command: echo $DEBUG
```

### Complex Workflow

```rust
// Runbook blocks in order:
// 1. Directory { path: "/app" }
// 2. Environment { name: "NODE_ENV", value: "production" }
// 3. SshConnect { user_host: "deploy@server.com" }
// 4. Script { code: "npm run build", output_variable: "build_result" }
// 5. Script { code: "echo 'Build: ${build_result}'" }

// Context evolution:
// After block 1: context.cwd = "/app"
// After block 2: context.env["NODE_ENV"] = "production"  
// After block 3: context.ssh_host = Some("deploy@server.com")
// Block 4 executes: ssh deploy@server.com "cd /app && NODE_ENV=production npm run build"
// Block 5 uses: build_result variable from previous execution
```

### Cancellation Example

```rust
// 1. Start long-running script
let handle = registry.execute_block(&script_block, context, ...).await?;

// 2. Store handle for later cancellation
state.block_executions.insert(handle.id, handle);

// 3. User clicks cancel button
// 4. Frontend calls cancel_block_execution(handle.id)
// 5. Backend finds handle and calls cancel()
handle.cancellation_token.cancel();

// 6. Running script receives signal and cleans up
// 7. Status updated to ExecutionStatus::Cancelled
```

## Architecture Benefits

### Type Safety
- **Compile-time guarantees** for block structure
- **No runtime type casting** or `Any` downcasting
- **Clear interfaces** between components

### Performance
- **Direct method dispatch** (no trait object overhead)
- **Async execution** doesn't block the main thread
- **Efficient cancellation** via tokio channels

### Maintainability
- **Clear separation** of data and behavior
- **Modular design** makes adding new blocks straightforward
- **Comprehensive testing** ensures reliability
- **Self-documenting** code with clear patterns

### Extensibility
- **Plugin-ready architecture** for future extensions
- **Context system** allows complex execution environments
- **Event system** for monitoring and debugging
- **Variable system** enables block chaining and workflows

This execution system provides a robust foundation for running complex, multi-step runbooks with proper error handling, cancellation, and state management.