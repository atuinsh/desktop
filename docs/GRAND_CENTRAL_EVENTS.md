# Atuin Desktop Grand Central Event System
This document provides a comprehensive overview of the Grand Central (GC) event system, which provides centralized, type-safe event handling across the entire Atuin Desktop application.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Core Components](#core-components)
- [Event Flow](#event-flow)
- [Event Types](#event-types)
- [Frontend Integration](#frontend-integration)
- [Backend Integration](#backend-integration)
- [Developer Guide](#developer-guide)
- [Examples & Patterns](#examples--patterns)

## Architecture Overview

The Grand Central event system is built around a **centralized event bus architecture** that maintains strict architectural boundaries while providing type-safe event communication between the backend runtime and frontend components.

### Key Design Principles

1. **Architectural Boundary Preservation**: Runtime layer has zero Tauri dependencies
2. **Type Safety**: Full TypeScript support with auto-generated bindings via ts-rs
3. **Centralized Events**: Single event system replaces scattered `app.emit()` calls
4. **Abstraction**: Runtime emits events through traits, not concrete implementations
5. **Extensibility**: Easy to add new event types and handlers

### System Layers

#### **Runtime Layer (Tauri-agnostic)**
- Defines `GCEvent` enum with all possible event types
- Uses `EventBus` trait for event emission
- No knowledge of Tauri or frontend implementation details

#### **Commands Layer (Tauri bridge)**
- Implements `EventBus` trait using Tauri channels
- Provides `subscribe_to_events` command for frontend subscription
- Maintains the architectural boundary between runtime and Tauri

#### **Frontend Layer**
- `GrandCentral` class extending Emittery for type-safe subscriptions
- Auto-generated TypeScript types from Rust via ts-rs
- Component-level event subscriptions with cleanup

## Core Components

### GCEvent Enum (Backend)

The `GCEvent` enum defines all events that can be emitted by the runtime:

```rust
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
#[serde(tag = "type", content = "data")]
pub enum GCEvent {
    /// PTY was opened and is ready for use
    PtyOpened(PtyMetadata),
    
    /// PTY was closed
    PtyClosed { pty_id: Uuid },
    
    /// Block execution started
    BlockStarted { block_id: Uuid, runbook_id: Uuid },
    
    /// Block execution finished successfully
    BlockFinished { block_id: Uuid, runbook_id: Uuid, success: bool },
    
    /// Block execution failed
    BlockFailed { block_id: Uuid, runbook_id: Uuid, error: String },
    
    /// Block execution was cancelled
    BlockCancelled { block_id: Uuid, runbook_id: Uuid },
    
    /// SSH connection established
    SshConnected { host: String, username: Option<String> },
    
    /// SSH connection failed
    SshConnectionFailed { host: String, error: String },
    
    /// SSH connection closed
    SshDisconnected { host: String },
    
    /// Runbook execution started
    RunbookStarted { runbook_id: Uuid },
    
    /// Runbook execution completed
    RunbookCompleted { runbook_id: Uuid },
    
    /// Runbook execution failed
    RunbookFailed { runbook_id: Uuid, error: String },
}
```

### EventBus Trait (Backend)

The `EventBus` trait abstracts event emission from the runtime:

```rust
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Emit an event to the event bus
    async fn emit(&self, event: GCEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

**Implementations:**
- `ChannelEventBus` - Production implementation using Tauri channels
- `NoOpEventBus` - No-op implementation for when events aren't needed
- `MemoryEventBus` - In-memory implementation for testing

### ExecutionContext Integration

The `ExecutionContext` now includes an event bus instead of direct Tauri dependencies:

```rust
pub struct ExecutionContext {
    pub runbook_id: Uuid,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub variables: HashMap<String, String>,
    pub ssh_host: Option<String>,
    pub document: Vec<serde_json::Value>,
    pub ssh_pool: Option<SshPoolHandle>,
    pub output_storage: Option<Arc<RwLock<HashMap<String, HashMap<String, String>>>>>,
    pub pty_store: Option<PtyStoreHandle>,
    pub event_bus: Option<Arc<dyn EventBus>>, // ← Replaces app_handle
}
```

### GrandCentral Class (Frontend)

The `GrandCentral` class provides type-safe event handling in the frontend:

```typescript
export class GrandCentral extends Emittery<GrandCentralEvents> {
  async startListening(): Promise<void>
  async stopListening(): Promise<void>
  get listening(): boolean
}
```

**Features:**
- Extends Emittery for robust event handling
- Type-safe event subscriptions
- Automatic backend event forwarding
- Lifecycle management (start/stop listening)

## Event Flow

### 1. Event Emission (Backend)

When a runtime component needs to emit an event:

```rust
// In TerminalHandler
if let Some(event_bus) = &context.event_bus {
    event_bus.emit(GCEvent::PtyOpened(metadata.clone())).await;
}
```

### 2. Event Transport (Commands Layer)

The `ChannelEventBus` forwards events to the frontend:

```rust
impl EventBus for ChannelEventBus {
    async fn emit(&self, event: GCEvent) -> Result<(), _> {
        self.sender.send(event)?;
        Ok(())
    }
}
```

### 3. Event Subscription (Frontend)

The frontend subscribes to the event stream:

```typescript
// Subscribe to backend events
await invoke('subscribe_to_events', {
  onEvent: (event: GCEvent) => {
    grandCentral.handleBackendEvent(event);
  }
});
```

### 4. Event Distribution (Frontend)

Events are distributed to component subscribers:

```typescript
// Component subscription
grandCentral.on('pty-opened', (data) => {
  console.log(`PTY opened: ${data.pty_id}`);
});
```

## Event Types

### PTY Events
- `PtyOpened` - PTY session started, includes metadata
- `PtyClosed` - PTY session ended

### Block Execution Events
- `BlockStarted` - Block execution began
- `BlockFinished` - Block execution completed successfully
- `BlockFailed` - Block execution failed with error
- `BlockCancelled` - Block execution was cancelled

### SSH Events
- `SshConnected` - SSH connection established
- `SshConnectionFailed` - SSH connection failed
- `SshDisconnected` - SSH connection closed

### Runbook Events
- `RunbookStarted` - Runbook execution began
- `RunbookCompleted` - Runbook execution finished
- `RunbookFailed` - Runbook execution failed

## Frontend Integration

### Initialization

Start the Grand Central system at application startup:

```typescript
import { grandCentral } from '@/lib/events/grand_central';

// Start listening to backend events
await grandCentral.startListening();
```

### Component Subscriptions

Subscribe to specific events in components:

```typescript
import { onPtyOpened, onBlockStarted } from '@/lib/events/grand_central';

// Using convenience functions
const unsubscribePty = onPtyOpened((data) => {
  console.log(`PTY ${data.pty_id} opened for block ${data.block}`);
});

// Using direct subscription
const unsubscribeBlock = grandCentral.on('block-started', (data) => {
  setBlockStatus(data.block_id, 'running');
});

// Cleanup
useEffect(() => {
  return () => {
    unsubscribePty();
    unsubscribeBlock();
  };
}, []);
```

### Type Safety

All events are fully typed with auto-generated TypeScript definitions:

```typescript
// Type-safe event data
grandCentral.on('block-failed', (data) => {
  // data is typed as: { block_id: string; runbook_id: string; error: string }
  showErrorToast(`Block ${data.block_id} failed: ${data.error}`);
});
```

## Backend Integration

### Event Emission in Handlers

Block handlers emit events through the context:

```rust
impl BlockHandler for ScriptHandler {
    async fn execute(&self, block: Script, context: ExecutionContext, ...) -> Result<ExecutionHandle, _> {
        // Emit start event
        if let Some(event_bus) = &context.event_bus {
            event_bus.emit(GCEvent::BlockStarted {
                block_id: block.id,
                runbook_id: context.runbook_id,
            }).await;
        }

        // ... execution logic ...

        // Emit completion event
        if let Some(event_bus) = &context.event_bus {
            event_bus.emit(GCEvent::BlockFinished {
                block_id: block.id,
                runbook_id: context.runbook_id,
                success: true,
            }).await;
        }

        Ok(handle)
    }
}
```

### Event Bus Setup

The commands layer provides the event bus implementation:

```rust
// In execute_block command
let gc_sender = state.gc_event_sender();
let event_bus = std::sync::Arc::new(ChannelEventBus::new(gc_sender));
context.event_bus = Some(event_bus);
```

## Developer Guide

### Adding New Event Types

1. **Add to GCEvent enum**:
```rust
pub enum GCEvent {
    // ... existing events
    MyNewEvent { field1: String, field2: i32 },
}
```

2. **Regenerate TypeScript bindings**:
```bash
pnpm run generate-bindings
```

3. **Add frontend event mapping**:
```typescript
export interface GrandCentralEvents {
  // ... existing events
  'my-new-event': { field1: string; field2: number };
}

// In handleBackendEvent
case 'MyNewEvent':
  this.emit('my-new-event', {
    field1: event.data.field1,
    field2: event.data.field2
  });
  break;
```

4. **Add convenience function** (optional):
```typescript
export const onMyNewEvent = (handler: (data: GrandCentralEvents['my-new-event']) => void) => 
  grandCentral.on('my-new-event', handler);
```

### Testing Event Emission

Use the `MemoryEventBus` for testing:

```rust
#[tokio::test]
async fn test_event_emission() {
    let event_bus = Arc::new(MemoryEventBus::new());
    let mut context = ExecutionContext::default();
    context.event_bus = Some(event_bus.clone());

    // ... perform operations that should emit events ...

    let events = event_bus.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        GCEvent::BlockStarted { block_id, .. } => {
            assert_eq!(*block_id, expected_block_id);
        }
        _ => panic!("Unexpected event type"),
    }
}
```

### Error Handling

Events are fire-and-forget to prevent blocking execution:

```rust
if let Some(event_bus) = &context.event_bus {
    event_bus.emit(event).await; // Don't propagate errors
}
```

## Examples & Patterns

### PTY Lifecycle Tracking

```typescript
// Track PTY sessions
const ptyStore = new Map<string, PtySession>();

onPtyOpened((data) => {
  ptyStore.set(data.pty_id, {
    id: data.pty_id,
    runbook: data.runbook,
    block: data.block,
    createdAt: data.created_at,
    status: 'active'
  });
});

onPtyClosed((data) => {
  const session = ptyStore.get(data.pty_id);
  if (session) {
    session.status = 'closed';
  }
});
```

### Block Execution Monitoring

```typescript
// Monitor block execution across runbooks
const executionTracker = new Map<string, BlockExecution>();

onBlockStarted((data) => {
  executionTracker.set(data.block_id, {
    blockId: data.block_id,
    runbookId: data.runbook_id,
    status: 'running',
    startTime: Date.now()
  });
});

onBlockFinished((data) => {
  const execution = executionTracker.get(data.block_id);
  if (execution) {
    execution.status = data.success ? 'success' : 'failed';
    execution.endTime = Date.now();
  }
});
```

### SSH Connection Management

```typescript
// Track SSH connections
const sshConnections = new Set<string>();

onSshConnected((data) => {
  sshConnections.add(data.host);
  showNotification(`Connected to ${data.host}`);
});

onSshDisconnected((data) => {
  sshConnections.delete(data.host);
  showNotification(`Disconnected from ${data.host}`);
});
```

## Architecture Benefits

### Boundary Preservation
- **Runtime isolation**: Zero Tauri dependencies in runtime layer
- **Clean interfaces**: Event emission through traits, not concrete types
- **Testability**: Runtime can use mock event bus implementations

### Type Safety
- **Compile-time guarantees**: ts-rs ensures frontend types match backend
- **No runtime errors**: TypeScript catches event data mismatches
- **IDE support**: Full autocomplete and type checking

### Performance
- **Async emission**: Events don't block execution
- **Efficient transport**: Single channel for all events
- **Selective subscription**: Components only listen to relevant events

### Maintainability
- **Centralized**: All events flow through one system
- **Self-documenting**: Event types clearly define system behavior
- **Extensible**: Easy to add new events without breaking changes

### Developer Experience
- **Convenience functions**: Simple subscription patterns
- **Global instance**: Easy access from any component
- **Lifecycle management**: Automatic cleanup and error handling

This Grand Central event system provides a robust, type-safe foundation for event-driven communication while maintaining clean architectural boundaries and excellent developer experience.
