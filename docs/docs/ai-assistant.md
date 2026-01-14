---
description: Use the AI Assistant to create, edit, and understand your runbooks through natural language.
---

# :material-robot: AI Assistant

The AI Assistant helps you create and edit runbooks through natural language conversation. Ask it to add blocks, modify content, explain what a runbook does, or generate entire workflows from a description.

## Opening the Assistant

Click the **sparkles icon** (:material-shimmer:) in the runbook editor toolbar to open the AI Assistant panel. The assistant appears as a chat interface alongside your runbook.

!!! info "Session Persistence"
    Your conversation history is saved and restored when you reopen the assistant for the same runbook.

## What the Assistant Can Do

The AI Assistant has tools to read and modify your runbook directly:

### Read Your Runbook

The assistant can examine your current document to understand its structure, answer questions about it, or plan edits. Ask things like:

- "What does this runbook do?"
- "Which blocks use the `environment` variable?"
- "Explain the script in the third block"

### Insert New Blocks

Add new content anywhere in your runbook:

- "Add a script block that checks disk usage"
- "Insert a PostgreSQL query block after the variables section"
- "Add a heading and explanation before the deployment steps"

### Update Existing Blocks

Modify blocks that are already in your runbook:

- "Change the SQL query to filter by status"
- "Update the script to use bash instead of zsh"
- "Fix the syntax error in the HTTP block"

### Replace or Remove Blocks

Restructure your runbook by replacing or deleting blocks:

- "Replace these three script blocks with a single combined one"
- "Remove the deprecated cleanup section"
- "Rewrite this section to use variables instead of hardcoded values"

## Tool Approval

When the assistant wants to modify your runbook, you'll see a tool approval prompt showing what it intends to do. You can:

- **Allow** - Execute this specific tool use
- **Always Allow** - Auto-approve future uses of this tool type in the current session
- **Deny** - Reject the change

!!! tip "Read Operations Auto-Approve"
    Reading your runbook document and fetching block documentation are automatically approved since they don't modify anything.

## Example Prompts

Here are some effective ways to use the assistant:

### Creating New Runbooks

> "Create a runbook for deploying a Node.js app to production. Include steps for building, running tests, and deploying to our server via SSH."

> "Generate a database maintenance runbook with health checks, backup verification, and cleanup queries for PostgreSQL."

### Enhancing Existing Runbooks

> "Add error handling to the deployment script - it should check if the build succeeded before continuing."

> "Make this runbook more reusable by converting the hardcoded server name to a variable."

### Understanding Runbooks

> "Walk me through what this runbook does step by step."

> "What would happen if the SSH connection fails during serial execution?"

## Best Practices

### Be Specific

The more context you provide, the better the results:

| Instead of... | Try... |
|---------------|--------|
| "Add a script" | "Add a bash script that lists running Docker containers" |
| "Fix this" | "The PostgreSQL query has a syntax error on line 3" |
| "Make it better" | "Add comments explaining what each script block does" |

### Work Incrementally

For complex runbooks, build up gradually:

1. Start with an outline or basic structure
2. Ask the assistant to flesh out each section
3. Review and refine as you go

### Use Block Names

When referencing specific blocks, use their names if they have them:

> "Update the 'fetch-metrics' script block to also capture memory usage"

### Let It Read First

For edits to existing content, the assistant works best when it reads the current state first. It will typically do this automatically, but you can explicitly ask:

> "Read the runbook and then update all the SQL blocks to use the new table name"

## Limitations

- The assistant cannot execute blocks or see their output - it only edits the runbook structure
- Very large runbooks may need to be edited in sections
- Complex block configurations may require manual fine-tuning after generation

## Troubleshooting

**Assistant seems confused about the runbook content**
Ask it to re-read the document: "Please read the current runbook and tell me what you see."

**Generated blocks have errors**
The assistant fetches block documentation before creating blocks, but complex configurations may need adjustment. Check the block's documentation for the correct format.

**Changes didn't appear**
Make sure you approved the tool use. Check for any error messages in the assistant's response.
