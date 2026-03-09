# Atuin MCP Server

MCP server that gives AI agents full access to Atuin's shell history, scripts, dotfiles, KV store, and Desktop app runbooks/workspaces.

## How it works

```
AI Agent (Claude, Cursor, etc.)
    ↕  stdio (JSON-RPC)
Atuin MCP Server (Bun + TypeScript)
    ↕           ↕
atuin CLI    Desktop App SQLite DB + Filesystem
```

- **CLI tools** — spawn `atuin <command>` and parse output
- **Desktop tools** — read/write `.atrb` YAML runbook files on disk and register workspaces in the desktop app's SQLite database (`~/Library/Application Support/sh.atuin.app/runbooks.db`)
- **Block execution tools** — call the desktop app's local HTTP API (requires the app to be running)

## Setup

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "atuin": {
      "command": "bun",
      "args": ["run", "/path/to/desktop/mcp-server/src/index.ts"],
      "env": {
        "ATUIN_MCP_DEV": "1"
      }
    }
  }
}
```

### Claude Code

Add `.mcp.json` to the project root:

```json
{
  "mcpServers": {
    "atuin": {
      "command": "bun",
      "args": ["run", "/path/to/desktop/mcp-server/src/index.ts"]
    }
  }
}
```

### Environment variables


| Variable            | Description                                                                  |
| ------------------- | ---------------------------------------------------------------------------- |
| `ATUIN_MCP_DEV`     | Set to `1` to use the dev database (`dev_runbooks.db`) instead of production |
| `ATUIN_MCP_DB_PATH` | Override the full path to the SQLite database                                |


## Tools (35 total)

### History (5 tools)


| Tool             | Description                                                                         | Parameters                                                                         |
| ---------------- | ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `history_search` | Search shell history with filters. Supports prefix, fulltext, fuzzy, and skim modes | `query` (required), `cwd`, `exit`, `before`, `after`, `limit`, `offset`, `reverse` |
| `history_list`   | List recent shell history entries with time, directory, duration, exit code         | `limit`, `offset`, `cwd`, `session`, `reverse`                                     |
| `history_last`   | Get the most recently executed shell command                                        | —                                                                                  |
| `history_stats`  | Usage statistics — most used commands, total count, unique commands                 | `date` (optional, e.g. "last friday")                                              |
| `history_delete` | Delete a specific command from history by ID                                        | `id`                                                                               |


### Scripts (4 tools)

Atuin scripts are shareable, syncable shell snippets with Jinja-style `{{variable}}` templates, E2E encrypted.


| Tool           | Description                                             | Parameters                                |
| -------------- | ------------------------------------------------------- | ----------------------------------------- |
| `scripts_list` | List all scripts                                        | —                                         |
| `scripts_get`  | Get script content and details                          | `name`                                    |
| `scripts_run`  | Execute a script, optionally passing template variables | `name`, `variables` (key-value map)       |
| `scripts_new`  | Create a new script from content or from last command   | `name`, `content`, `shebang`, `from_last` |


### Dotfiles (6 tools)

Synced and encrypted shell aliases and environment variables.


| Tool           | Description                    | Parameters                   |
| -------------- | ------------------------------ | ---------------------------- |
| `alias_list`   | List all shell aliases         | —                            |
| `alias_set`    | Create or update an alias      | `name`, `command`            |
| `alias_delete` | Delete an alias                | `name`                       |
| `var_list`     | List all environment variables | —                            |
| `var_set`      | Set an environment variable    | `name`, `value`, `no_export` |
| `var_delete`   | Delete an environment variable | `name`                       |


### KV Store (2 tools)

Encrypted, synced key-value store. Max 100KiB per entry.


| Tool     | Description          | Parameters                  |
| -------- | -------------------- | --------------------------- |
| `kv_get` | Get a value by key   | `key`, `namespace`          |
| `kv_set` | Set a key-value pair | `key`, `value`, `namespace` |


### Sync (2 tools)


| Tool          | Description                                                 | Parameters |
| ------------- | ----------------------------------------------------------- | ---------- |
| `sync`        | Push and pull encrypted history, scripts, dotfiles, KV data | `force`    |
| `sync_status` | Check sync and connection status                            | —          |


### Store (3 tools)


| Tool            | Description                                | Parameters             |
| --------------- | ------------------------------------------ | ---------------------- |
| `store_status`  | Check the status of the local record store | —                      |
| `store_verify`  | Verify store integrity                     | —                      |
| `store_rebuild` | Rebuild the local store from records       | `tag` (e.g. "history") |


### Info (2 tools)


| Tool     | Description                                                           | Parameters |
| -------- | --------------------------------------------------------------------- | ---------- |
| `doctor` | Run diagnostics for shell integration, sync, config issues            | —          |
| `info`   | System and configuration info — version, data directory, sync address | —          |


### Workspaces (3 tools)

Workspaces are directories registered in the desktop app, containing runbook files.


| Tool               | Description                                                       | Parameters   |
| ------------------ | ----------------------------------------------------------------- | ------------ |
| `workspace_list`   | List all workspaces with runbook counts                           | —            |
| `workspace_get`    | Get workspace details and its runbooks                            | `name_or_id` |
| `workspace_create` | Create a new workspace (directory + atuin.toml + DB registration) | `name`       |


### Runbooks (5 tools)

Runbooks are `.atrb` YAML files containing executable blocks (scripts, HTTP requests, SQL queries, etc.).


| Tool             | Description                                        | Parameters                               |
| ---------------- | -------------------------------------------------- | ---------------------------------------- |
| `runbook_list`   | List all runbooks across all workspaces            | —                                        |
| `runbook_get`    | Read a runbook's full content including all blocks | `name_or_id`                             |
| `runbook_create` | Create a new runbook with blocks in a workspace    | `name`, `workspace_name_or_id`, `blocks` |
| `runbook_update` | Update a runbook — rename or replace blocks        | `name_or_id`, `new_name`, `blocks`       |
| `runbook_delete` | Delete a runbook                                   | `name_or_id`                             |


#### Block types for runbooks

When creating or updating runbooks, `blocks` is an array of `{type, props}` objects:


| Block type    | Props                                                                                              | Description             |
| ------------- | -------------------------------------------------------------------------------------------------- | ----------------------- |
| `heading`     | `level` (1-6), `textColor`, `backgroundColor`, `textAlignment`                                     | Section heading         |
| `paragraph`   | `textColor`, `backgroundColor`, `textAlignment`                                                    | Text paragraph          |
| `run`         | `type` ("bash"), `name`, `code`, `pty`, `global`, `outputVisible`, `dependency`                    | Terminal/bash command   |
| `script`      | `interpreter` ("zsh"/"bash"/"nodejs"/"python3"), `name`, `code`, `outputVariable`, `outputVisible` | Script with interpreter |
| `var`         | `name`, `value`                                                                                    | Template variable       |
| `env`         | `name`, `value`                                                                                    | Environment variable    |
| `directory`   | `path`                                                                                             | Set working directory   |
| `http`        | `method`, `url`, `headers`, `body`                                                                 | HTTP request            |
| `sqlite`      | `path`, `query`                                                                                    | SQLite query            |
| `postgres`    | `connection_string`, `query`                                                                       | PostgreSQL query        |
| `mysql`       | `connection_string`, `query`                                                                       | MySQL query             |
| `clickhouse`  | `connection_string`, `query`                                                                       | ClickHouse query        |
| `kubernetes`  | *(varies)*                                                                                         | Kubernetes operation    |
| `prometheus`  | `url`, `query`                                                                                     | Prometheus metric query |
| `ssh_connect` | `host`, `user`, `port`                                                                             | SSH connection          |
| `pause`       | `duration`                                                                                         | Pause execution         |
| `dropdown`    | `name`, `options`                                                                                  | Dropdown selector       |
| `sub_runbook` | `runbook_id` or `uri`                                                                              | Embed another runbook   |


### Block Execution (3 tools)

These require the desktop app to be running with the MCP API enabled.


| Tool            | Description                                           | Parameters                                     |
| --------------- | ----------------------------------------------------- | ---------------------------------------------- |
| `block_execute` | Execute a single block via the desktop runtime engine | `runbook_id`, `block_type`, `props`, `context` |
| `workflow_run`  | Run a sequence of blocks from a runbook               | `runbook_id`, `block_ids`                      |
| `workflow_stop` | Stop a running workflow                               | `runbook_id`                                   |


## Resources (6 total)

MCP resources provide read-only data that agents can browse.


| URI                        | Description                                 |
| -------------------------- | ------------------------------------------- |
| `atuin://history/recent`   | Last 50 shell commands                      |
| `atuin://scripts`          | All scripts                                 |
| `atuin://dotfiles/aliases` | All shell aliases                           |
| `atuin://dotfiles/vars`    | All environment variables                   |
| `atuin://workspaces`       | All workspaces and their runbooks           |
| `atuin://runbook/{id}`     | A specific runbook by ID or name (template) |


## File structure

```
mcp-server/
├── package.json
├── tsconfig.json
├── mcp.md              ← this file
└── src/
    ├── index.ts         # Entry point — server setup, tool + resource registration
    ├── cli.ts           # atuin CLI executor (Bun.spawn wrapper)
    ├── types.ts         # Shared TypeScript types
    ├── tools/
    │   ├── history.ts   # history_search, history_list, history_last, history_stats, history_delete
    │   ├── scripts.ts   # scripts_list, scripts_get, scripts_run, scripts_new
    │   ├── dotfiles.ts  # alias_list, alias_set, alias_delete, var_list, var_set, var_delete
    │   ├── kv.ts        # kv_get, kv_set
    │   ├── sync.ts      # sync, sync_status
    │   ├── store.ts     # store_status, store_verify, store_rebuild
    │   ├── info.ts      # doctor, info
    │   ├── runbooks.ts  # runbook_list, runbook_get, runbook_create, runbook_update, runbook_delete
    │   ├── workspaces.ts# workspace_list, workspace_get, workspace_create
    │   └── blocks.ts    # block_execute, workflow_run, workflow_stop
    └── desktop/
        ├── fs.ts        # Workspace/runbook filesystem + SQLite DB operations
        └── api.ts       # Desktop app HTTP client for block execution
```

## Runbook file format (.atrb)

Runbooks are YAML files stored in workspace directories:

```yaml
id: 09fe757d-657e-41c2-bcab-057d6b38b42e
name: Deploy Checklist
version: 1
content:
  - id: cd54558c-b1ca-4002-94f6-b94055f94e9e
    type: run
    props:
      type: bash
      name: Check git status
      code: git status && git log --oneline -5
      pty: ""
      global: false
      outputVisible: true
      dependency: "{}"
    content: []
    children: []
  - id: 1cf36910-9f72-4b85-8da3-3d3c2633ab6c
    type: var
    props:
      name: DEPLOY_ENV
      value: production
    content: []
    children: []
```

## Workspace structure

```
~/Documents/Atuin Runbooks/
├── My Workspace/
│   ├── atuin.toml          # [workspace] id = "..." name = "..."
│   ├── runbook-one.atrb
│   └── runbook-two.atrb
└── Another Workspace/
    ├── atuin.toml
    └── ops-playbook.atrb
```

Workspaces are also registered in `~/Library/Application Support/sh.atuin.app/runbooks.db` (or `dev_runbooks.db` for dev) so the desktop app can discover them