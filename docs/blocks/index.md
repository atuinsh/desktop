---
description: Interactive components that make runbooks executable with database, script, network, and monitoring blocks.
---

# :material-cube: Blocks

Blocks are the interactive building components that make Atuin Desktop runbooks come alive. They're executable elements designed specifically for operations and automation workflows.

## What are Blocks?

Just as document editors let you embed interactive elements, Atuin Desktop blocks let you embed executable components into your runbooks. The key difference? Our blocks execute real commands, query live databases, and automate workflows - built specifically for DevOps, SRE, and operations teams.

!!! example "From Static to Interactive"
    **Traditional documentation:**
    
    ```markdown
    1. SSH to the server: ssh user@prod-server
    2. Check disk usage: df -h
    3. Query the database: SELECT COUNT(*) FROM users;
    ```
    
    **Atuin Desktop runbook:**
    
    - :material-server-network: **SSH Block** - Actually connects to your server
    - :material-console: **Terminal Block** - Runs `df -h` and shows real output  
    - :simple-postgresql: **PostgreSQL Block** - Executes the query and displays results in a table

## How Blocks Work Together

Blocks in a runbook share context and can pass data between each other, creating powerful automation workflows.

### Contextual Blocks

These blocks set the environment and context for subsequent blocks:

- **:material-folder: Directory Block** - Sets the working directory for all following blocks
- **:material-variable: Environment Block** - Defines environment variables
- **:material-tag: Variable Block** - Creates template variables for reuse

!!! info "Cascading Context"
    When you set a directory with a Directory block, all subsequent Script, Terminal, and other executable blocks will run in that directory automatically.

### Executable Blocks

These blocks perform actions, run commands, or retrieve data:

- **:material-script: Script Block** - Executes code in various languages (bash, python, node)
- **:material-console: Terminal Block** - Provides interactive terminal sessions
- **:simple-postgresql: Database Blocks** - Query and manipulate databases
- **:material-web: HTTP Block** - Make API requests and web calls

### Template Variables

Blocks can capture output as variables and share data:

```handlebars
# A Script block captures server info
{{var.server_status}}

# An HTTP block uses that data
POST /alerts with body: {"server": "{{var.server_status}}"}
```

### Block Output {: #block-output }

Many blocks produce structured output that can be accessed in templates after they execute. This allows you to use results from one block in subsequent blocks.

#### Accessing Block Output

Block output is accessed through the `doc.named` object using the block's name:

```jinja
{{ doc.named['my_block_name'].output.field_name }}
```

A common pattern is to assign the output to a variable for easier access:

```jinja
{%- set output = doc.named['my_query'].output %}
Total rows: {{ output.total_rows }}
First result: {{ output.rows[0] }}
```

#### Blocks with Output

The following blocks produce structured output:

| Block | Output Type | Key Fields |
|-------|------------|------------|
| [SQL Databases](databases/index.md) | Query results | `rows`, `columns`, `total_rows` |
| [HTTP](network/http.md) | Response data | `status`, `body`, `body_json`, `headers` |
| [Script](executable/script.md) | Execution result | `exit_code`, `stdout`, `stderr`, `combined` |
| [Terminal](executable/terminal.md) | Terminal output | `output`, `byte_count`, `cancelled` |
| [Kubernetes](executable/kubernetes.md) | Resource data | `data`, `columns`, `item_count`, `resource_kind` |
| [Prometheus](monitoring/prometheus.md) | Metrics | `series`, `total_series`, `time_range` |

See each block's documentation for detailed output field descriptions.

## Block Examples

### :material-console: Terminal Block
Perfect for interactive debugging and exploration:

```bash
# Check system resources
top -n 1
free -h
df -h
```

### :material-script: Script Block  
Non-interactive execution with output capture for automation:

```bash
# Check system status and capture output as variable
echo "System: $(uname -s)"
echo "Load: $(uptime | awk -F'load average:' '{print $2}')"
echo "Memory: $(free -m | awk 'NR==2{printf "%.1f%%", $3*100/$2}')"
echo "Disk: $(df -h / | awk 'NR==2{print $5}')"
```

*Output gets saved to a variable and can be referenced in other blocks as `{{var.system_status}}`*

### :simple-postgresql: PostgreSQL Block
Query your databases directly in your runbooks:

```sql
SELECT 
    table_name,
    pg_size_pretty(pg_total_relation_size(table_name::regclass)) as size
FROM information_schema.tables 
WHERE table_schema = 'public'
ORDER BY pg_total_relation_size(table_name::regclass) DESC;
```

### :material-folder: Directory Block
Set context for file operations:

```
/var/log/myapp
```

Then all subsequent blocks run in that directory automatically.

## Block Categories

<div class="grid cards" markdown>

-   :material-database:{ .lg .middle } **Database Blocks**

    ---

    Connect to and query MySQL, PostgreSQL, ClickHouse, and SQLite databases directly from your runbooks.

    [:octicons-arrow-right-24: Explore Database Blocks](databases/index.md)

-   :material-play:{ .lg .middle } **Executable Blocks**

    ---

    Run scripts, execute commands, manage environments, and automate workflows with powerful execution blocks.

    [:octicons-arrow-right-24: Explore Executable Blocks](executable/README.md)

-   :material-network:{ .lg .middle } **Network Blocks**

    ---

    Make HTTP requests, establish SSH connections, and interact with remote systems and APIs.

    [:octicons-arrow-right-24: Explore Network Blocks](network/README.md)

-   :material-monitor:{ .lg .middle } **Monitoring Blocks**

    ---

    Query monitoring systems like Prometheus and integrate real-time metrics into your runbooks.

    [:octicons-arrow-right-24: Explore Monitoring Blocks](monitoring/README.md)

</div>

## Getting Started with Blocks

1. **Create a block** - Type `/` anywhere in your runbook to open the block picker
2. **Configure the block** - Fill in connection details, queries, or commands  
3. **Use template variables** - Make blocks dynamic with `{{var.name}}` syntax
4. **Chain blocks together** - Use output from one block as input to another
5. **Run your workflow** - Execute individual blocks or run the entire runbook
