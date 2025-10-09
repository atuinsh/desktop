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

## Variable Display

Use the Variable Display block to view all currently set variables and their values. This shows both template (synced) and local (not synced) variables for debugging and state inspection.

### Usage

Both variable types can be referenced using the same template syntax:

```handlebars
{{var.variable_name}}
```

### Template Integration

All variable blocks integrate with the [templating](../../templating.md "mention") system, enabling complex variable manipulation and conditional logic.
