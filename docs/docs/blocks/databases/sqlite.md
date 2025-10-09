---
description: Connect to local SQLite database files and execute SQL queries
---

# :simple-sqlite: SQLite

The SQLite block allows you to connect to SQLite databases and execute queries directly within your runbook.

## Connection

Configure your SQLite connection using the database file path:

- **Database Path** - Full path to your SQLite database file (e.g., `/path/to/database.db`)

SQLite is perfect for local development, testing, and lightweight applications that don't require a separate database server. SQLite databases are stored as single files on your filesystem, making them easy to backup, share, and version control.

## Query Execution

Write and execute SQL queries against your SQLite database. Results are displayed in a table format within the runbook. SQLite supports most standard SQL features including:

- Transactions and ACID compliance
- Indexes and views
- Triggers and foreign keys
- Common table expressions (CTEs)
- JSON functions (in newer versions)
- Full-text search with FTS extensions

## Template Usage

All input fields are first rendered by the [templating](../../templating.md) system, allowing you to use variables in your queries and connection parameters.

```sql
SELECT * FROM logs 
WHERE level = '{{var.log_level}}' 
AND timestamp > datetime('{{var.start_time}}');
```

You can also use template variables for database paths to switch between different database files:

```
Database Path: {{var.db_path}}/{{var.environment}}.db
```

## Security

Consider using [secrets](../../secrets.md) for sensitive database file paths or if your SQLite database uses encryption.

!!! warning "File Permissions"
    - Ensure proper file system permissions on your SQLite database files
    - Be careful with database files in version control (consider using .gitignore)
    - Use encrypted SQLite databases for sensitive data
    - Regular backups are important since SQLite databases are single files
