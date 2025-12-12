# Pause

The pause block halts serial workflow execution at a designated point, allowing you to perform manual tasks before resuming. This is useful for workflows that require human intervention, verification, or approval steps.

## Pause Modes

The pause block supports two modes:

### Always Pause

When set to "Always", the workflow will unconditionally stop at this block during serial execution. Click the play button on the block to continue.

### Conditional Pause

When set to "If condition", you can specify a MiniJinja expression that determines whether the workflow should pause. The workflow only pauses if the expression evaluates to a truthy value.

```handlebars
{{ var.requires_approval }}
```

Truthy values include: `true`, `"true"`, `"1"`, `"yes"`, or any non-zero number.

## Resuming Execution

When a workflow is paused:

1. The pause block's icon changes to a glowing green play button
2. A notification sound plays (configurable in Settings)
3. If the app is not focused, a system notification appears

Click the play button on the pause block to resume execution from the next block.

## CLI Behavior

When running workflows via the CLI:

- **Interactive mode**: The CLI will display "Workflow paused. Press Enter to continue..." and wait for user input
- **Non-interactive mode**: The workflow will exit with an error when a pause block is triggered

## Example Use Cases

- **Deployment workflows**: Pause after staging deployment to verify before proceeding to production
- **Data migrations**: Pause to verify data integrity between migration steps
- **Approval gates**: Pause for manual sign-off before critical operations
- **Debugging**: Insert temporary pauses to inspect intermediate state

## Template Usage

Conditional pauses can reference any template variable:

```handlebars
{{ var.environment == "production" }}
{{ var.skip_confirmation != "true" }}
{{ var.approval_required }}
```

All condition fields are rendered by the [templating](../../templating.md) system.
