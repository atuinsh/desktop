---
description: Query Postgresql databases, and render the results in a runbook
---

# :simple-postgresql: PostgreSQL

The PostgreSQL block allows you to connect to PostgreSQL databases and execute queries directly within your runbook.

## Connection

Configure your PostgreSQL connection using a standard PostgreSQL connection URI:

```
postgresql://username:password@host:port/database
```

!!! info "URI Only"
    Currently, PostgreSQL blocks only support connection URI format. Individual connection parameters (host, port, etc.) are not supported at this time.

## Query Execution

Write and execute SQL queries against your PostgreSQL database. Results are displayed in a table format within the runbook. PostgreSQL supports advanced SQL features like:

- Common Table Expressions (CTEs)
- Window functions
- JSON/JSONB operations
- Array operations
- Full-text search

## Template Usage

All input fields are first rendered by the [templating](../../templating.md) system, allowing you to use variables in your queries and connection parameters.

```sql
SELECT * FROM users WHERE created_at > '{{var.start_date}}' 
AND department = '{{var.department}}';
```

Use template variables to create flexible queries that adapt based on runbook inputs or previous block outputs.

## Block Output

PostgreSQL blocks produce structured output that can be accessed in templates. See [Database Block Output](index.md#block-output) for full documentation on available fields and usage examples.

```jinja
{%- set output = doc.named['my_pg_query'].output %}
Found {{ output.total_rows }} rows
```

## Security

Consider using [secrets](../../secrets.md) for sensitive connection parameters like passwords to avoid storing credentials in plain text.

!!! warning "Security Best Practices"
    - Never hardcode credentials in your runbooks
    - Use environment variables or the secrets system
    - Consider using read-only database users for monitoring queries
    - Be cautious with queries that modify data (INSERT, UPDATE, DELETE)
