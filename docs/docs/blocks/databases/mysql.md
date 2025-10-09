---
description: Query MySQL databases, and render the results in a runbook
---

# :simple-mysql: MySQL

The MySQL block allows you to connect to MySQL databases and execute queries directly within your runbook.

## Connection

Configure your MySQL connection using a standard MySQL connection URI:

```
mysql://username:password@host:port/database
```

!!! info "URI Only"
    Currently, MySQL blocks only support connection URI format. Individual connection parameters (host, port, etc.) are not supported at this time.

MySQL blocks support SSL connections for secure database communication. Use SSL parameters in your connection URI (e.g., `?ssl-mode=required`).

## Query Execution

Write and execute SQL queries against your MySQL database. Results are displayed in a table format within the runbook. MySQL supports various SQL features including:

- Stored procedures and functions
- Views and temporary tables
- Joins and subqueries
- Aggregate functions and GROUP BY
- Full-text search with MATCH/AGAINST

## Template Usage

All input fields are first rendered by the [templating](../../templating.md) system, allowing you to use variables in your queries and connection parameters.

```sql
SELECT * FROM users WHERE status = '{{var.user_status}}' 
AND created_date > '{{var.start_date}}';
```

Use template variables to make your queries flexible and reusable across different environments or conditions.

## Security

Consider using [secrets](../../secrets.md) for sensitive connection parameters like passwords to avoid storing credentials in plain text.

!!! warning "Security Best Practices"
    - Never hardcode database passwords in runbooks
    - Use read-only database users when possible
    - Enable SSL/TLS encryption for production connections
    - Be careful with data modification queries (UPDATE, DELETE, DROP)
