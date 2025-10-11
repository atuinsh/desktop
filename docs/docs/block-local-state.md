# Block Local State

This document describes the third data persistence option for blocks in Atuin runbooks.

## Overview

Blocks in Atuin runbooks now have three ways to persist data:

1. **React State** - Stored in component state, lost on reload
2. **Block Props** - Stored in block schema, synced across all users
3. **Block Local State** (NEW) - Stored in SQLite, persists across reloads but stays local to the user

## When to Use Block Local State

Use block local state when you need to:

- Store UI preferences per block (e.g., collapsed state, selected tab)
- Persist user-specific settings that shouldn't sync to other users
- Cache data that needs to survive page reloads
- Store temporary block-specific data that's not part of the runbook content

## Usage

### Basic Example

```typescript
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";

interface MyBlockComponentProps {
  block: { id: string; props: any };
  editor: any;
}

function MyBlockComponent({ block, editor }: MyBlockComponentProps) {
  // Works just like useState, but persists across reloads
  const [collapsed, setCollapsed] = useBlockLocalState<boolean>(
    block.id,
    "collapsed",
    false
  );

  return (
    <div>
      <button onClick={() => setCollapsed(!collapsed)}>
        {collapsed ? "Expand" : "Collapse"}
      </button>
      {!collapsed && <div>Block content...</div>}
    </div>
  );
}
```

### Supported Types

The hook supports three types: `string`, `number`, and `boolean`.

```typescript
// String - explicitly typed to allow any string value
const [selectedTab, setSelectedTab] = useBlockLocalState<string>(
  block.id,
  "selectedTab",
  "preview"
);

// Number - explicitly typed to allow any number value
const [count, setCount] = useBlockLocalState<number>(
  block.id,
  "count",
  0
);

// Boolean - explicitly typed to allow any boolean value
const [enabled, setEnabled] = useBlockLocalState<boolean>(
  block.id,
  "enabled",
  true
);
```

### Real-World Example: Collapsible Code Block

```typescript
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";
import { ChevronDown, ChevronRight } from "lucide-react";

interface CodeBlockProps {
  block: { id: string; props: { code: string } };
  editor: any;
}

function CodeBlock({ block, editor }: CodeBlockProps) {
  const [collapsed, setCollapsed] = useBlockLocalState<boolean>(
    block.id,
    "collapsed",
    false
  );

  return (
    <div className="code-block">
      <div
        className="code-header"
        onClick={() => setCollapsed(!collapsed)}
      >
        {collapsed ? <ChevronRight /> : <ChevronDown />}
        <span>Code</span>
      </div>
      {!collapsed && (
        <pre>
          <code>{block.props.code}</code>
        </pre>
      )}
    </div>
  );
}
```

## Implementation Details

### Backend

Data is stored in the `block_local_state` SQLite table in the `runbooks` database:

```sql
CREATE TABLE block_local_state (
    runbook_id TEXT NOT NULL,
    block_id TEXT NOT NULL,
    property_name TEXT NOT NULL,
    property_value TEXT NOT NULL,
    created BIGINT NOT NULL,
    updated BIGINT NOT NULL,
    PRIMARY KEY (runbook_id, block_id, property_name)
);
```

### Frontend

The hook automatically:
- Loads the value from the database on mount
- Saves the value to the database when it changes
- Handles type conversions (string, number, boolean)
- Associates the value with the current runbook and block ID

### API

Low-level API functions are available if you need more control:

```typescript
import {
  setBlockLocalState,
  getBlockLocalState,
  getBlockLocalStateAll,
  deleteBlockLocalState,
  deleteBlockLocalStateAll,
} from "@/state/block_state";

// Set a property
await setBlockLocalState(runbookId, blockId, "collapsed", "true");

// Get a property
const value = await getBlockLocalState(runbookId, blockId, "collapsed");

// Get all properties for a block
const allProps = await getBlockLocalStateAll(runbookId, blockId);

// Delete a property
await deleteBlockLocalState(runbookId, blockId, "collapsed");

// Delete all properties for a block
await deleteBlockLocalStateAll(runbookId, blockId);
```

## Migration

The database migration is automatically applied when the application starts. The migration files are:

- `backend/migrations/runbooks/20251011000000_create_block_local_state.up.sql`
- `backend/migrations/runbooks/20251011000000_create_block_local_state.down.sql`

## Comparison with Other Persistence Options

| Feature | React State | Block Props | Block Local State |
|---------|------------|-------------|-------------------|
| Persists across reloads | ❌ | ✅ | ✅ |
| Syncs across users | N/A | ✅ | ❌ |
| Use case | Temporary UI state | Runbook content | User-specific preferences |
| Storage | Memory | Runbook document | SQLite database |
| Performance | Fastest | Medium | Medium |

## Best Practices

1. **Use for UI state only** - Don't store runbook content in local state
2. **Keep properties simple** - Use simple string/number/boolean values
3. **Use descriptive property names** - e.g., "collapsed", "selectedTab", "viewMode"
4. **Provide sensible defaults** - Always provide a default value that makes sense
5. **Clean up when appropriate** - Consider using `deleteBlockLocalStateAll` when a block is deleted

## Examples in the Codebase

To see block local state in action, check out these examples:

- Collapsible code blocks (Issue #85)
- Tabbed interfaces that remember the selected tab
- Expandable/collapsible sections
- User-specific view preferences

## Testing

To test the implementation:

1. Create a block that uses `useBlockLocalState`
2. Change the value (e.g., collapse a section)
3. Reload the page
4. Verify the state persists (e.g., the section is still collapsed)
5. Open the same runbook on another device or user account
6. Verify the state is NOT synced (e.g., the section starts expanded)

