# Block Type System

## Shared Types (Backend → Frontend)

The block execution system uses TypeScript-generated types for frontend communication via ts-rs.

### Core Types

#### BlockOutput
Sent through Tauri channels during block execution:
- `stdout`: Optional string output from the block
- `stderr`: Optional error output from the block  
- `lifecycle`: Optional lifecycle event

#### BlockLifecycleEvent
Discriminated union for block execution lifecycle:
- `Started`: Block execution has begun
- `Finished(BlockFinishedData)`: Block completed with exit code and success status
- `Cancelled`: User cancelled the execution
- `Error(BlockErrorData)`: Execution failed with error message

#### ExecutionStatus
Represents the current state of a block execution:
- `Running`: Block is currently executing
- `Success(String)`: Block completed successfully with output
- `Failed(String)`: Block failed with error message
- `Cancelled`: Block execution was cancelled

### Type Generation

Run one of the following to regenerate TypeScript types after changes:
```bash
cargo test
# or
pnpm generate-bindings
```

Generated types will be placed in the `bindings/` directory.

### Frontend Usage

The generated types use discriminated unions for type-safe event handling:

```typescript
// Handle lifecycle events with exhaustive checks
switch (event.lifecycle.type) {
  case "Started":
    console.log("Block execution started");
    break;
  case "Finished":
    console.log(`Exit code: ${event.lifecycle.data.exit_code}`);
    console.log(`Success: ${event.lifecycle.data.success}`);
    break;
  case "Cancelled":
    console.log("Block execution cancelled");
    break;
  case "Error":
    console.error(`Error: ${event.lifecycle.data.message}`);
    break;
  default:
    // TypeScript ensures all cases are handled
    const exhaustiveCheck: never = event.lifecycle;
    throw new Error(`Unhandled lifecycle event: ${exhaustiveCheck}`);
}
```

### Benefits

- **Type Safety**: Frontend and backend types are always in sync
- **Exhaustive Checks**: TypeScript compiler ensures all enum cases are handled
- **IDE Support**: Full autocomplete and type checking in your editor
- **Zero Runtime Cost**: Types are compile-time only
- **Automatic Updates**: Changes to Rust types automatically update TypeScript

### Adding New Types

To add a new shared type:

1. Add `#[derive(TS)]` and `#[ts(export)]` to your Rust type
2. For enums, use `#[ts(tag = "type", content = "data")]` for discriminated unions
3. Run `cargo test` or `pnpm generate-bindings` to generate TypeScript
4. Import and use the generated types in your frontend code

Example:
```rust
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct MyNewType {
    pub field: String,
}

#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
pub enum MyNewEnum {
    VariantA(String),
    VariantB { field: i32 },
}
```