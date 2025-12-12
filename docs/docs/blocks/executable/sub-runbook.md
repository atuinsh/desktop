# :material-book-open-variant: Sub-Runbook

The sub-runbook block lets you embed and run another runbook as part of your workflow. Build complex automation by composing smaller, reusable runbooks.

## Adding a Sub-Runbook

Type `/subrunbook` in the editor to insert a sub-runbook block, then click the dropdown to select which runbook to embed.

### Local Runbooks

Search by name to find runbooks across your workspaces. Your most recent runbooks appear by default.

### Hub Runbooks

Paste a Hub URI to load a runbook from Atuin Hub:

```
owner/runbook
owner/runbook:v1.0
```

Hub search is coming soon.

## Version Tags

For Hub runbooks, a tag selector appears next to the dropdown. Pin to a specific version for stability, or use `latest` for the most recent.

## Running

Click the play button to execute. Progress shows how many blocks have completed and which block is currently running.

Sub-runbooks inherit environment variables and template variables from the parent, and any variables they set are available to subsequent blocks.

## Use Cases

- **Shared setup**: Reuse common initialization across multiple runbooks
- **Team libraries**: Pull in runbooks maintained by your team on the Hub
- **Modular workflows**: Break complex processes into manageable pieces
- **Versioned steps**: Pin critical operations to tested versions
