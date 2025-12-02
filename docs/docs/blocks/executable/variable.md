# Variable

The variable system supports three different types of variables, each with different scoping and synchronization behavior.

## Template Variables (Var Block)

Template variables are **synced** across all collaborators in real-time. These are the primary variable type for shared runbook state.

- **Synchronization**: Changes are immediately visible to all collaborators
- **Persistence**: Stored with the runbook document  
- **Scope**: Available throughout the entire runbook
- **Use case**: Shared configuration, user inputs, persistent state

Variables can be set with a name and value. The value field supports full [templating](../../templating.md "mention") syntax, allowing you to build variables from other variables or block outputs.

## Local Variables (Local Var Block)

Local variables are **not synced** and remain private to each user's session.

- **Synchronization**: Private to individual users, not shared
- **Persistence**: Lost when the session ends
- **Scope**: Available throughout the runbook for that user only
- **Use case**: User-specific settings, temporary calculations, private credentials

## Setting Variables from Script and Terminal Blocks

[Script](script.md) and [Terminal](terminal.md) blocks can set multiple template variables by writing to the `$ATUIN_OUTPUT_VARS` file. This provides a programmatic way to create variables based on command execution.

**Usage:**

```bash
echo "name=value" >> $ATUIN_OUTPUT_VARS
```

- **Format**: `KEY=VALUE` entries, one per line
- **Timing**: Variables are captured when scripts exit successfully or terminals close
- **Location**: Works with both local and remote (SSH) execution

See the [Script](script.md#setting-variables-via-atuin_output_vars) and [Terminal](terminal.md#variables) documentation for detailed examples.

## Variable Display

Use the Variable Display block to view all currently set variables and their values. This shows both template (synced) and local (not synced) variables for debugging and state inspection.

### Usage

Both variable types can be referenced using the same template syntax:

```handlebars
{{var.variable_name}}
```

### Template Integration

All variable blocks integrate with the [templating](../../templating.md "mention") system, enabling complex variable manipulation and conditional logic.
