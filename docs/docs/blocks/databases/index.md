---
description: Query MySQL, PostgreSQL, ClickHouse, and SQLite databases directly in runbooks.
---

# :material-database: Database Blocks

Database blocks allow you to integrate database querying with your runbook. All input fields are first run through the [templating](../../templating.md) system, making it easy to parameterize your database operations.

## Available Database Blocks

<div class="grid cards" markdown>

-   :simple-clickhouse:{ .lg .middle } **ClickHouse**

    ---

    Connect to ClickHouse databases for analytics and OLAP queries.

    [:octicons-arrow-right-24: Learn more](clickhouse.md)

-   :simple-mysql:{ .lg .middle } **MySQL**

    ---

    Connect to MySQL databases with full SQL query support.

    [:octicons-arrow-right-24: Learn more](mysql.md)

-   :simple-postgresql:{ .lg .middle } **PostgreSQL**

    ---

    Connect to PostgreSQL databases with advanced features.

    [:octicons-arrow-right-24: Learn more](postgresql.md)

-   :simple-sqlite:{ .lg .middle } **SQLite**

    ---

    Work with local SQLite databases for lightweight operations.

    [:octicons-arrow-right-24: Learn more](sqlite.md)

</div>

## Block Output {: #block-output }

All SQL database blocks produce structured output that can be accessed in templates after execution. See [Block Output](../index.md#block-output) for general information on accessing block output.

### Accessing Query Results

```jinja
{%- set output = doc.named['my_query'].output %}

{# Access rows from SELECT queries #}
{% for row in output.rows %}
  {{ row.column_name }}
{% endfor %}
```

### Output Fields

| Field | Type | Description |
|-------|------|-------------|
| `results` | array | All query results (for multi-statement queries) |
| `first` | object | The first result (convenience accessor) |
| `rows` | array | Rows from the first SELECT query |
| `columns` | array | Column definitions from the first SELECT query |
| `total_rows` | number | Total row count across all SELECT results |
| `total_rows_affected` | number | Total rows affected by INSERT/UPDATE/DELETE |
| `rows_affected` | number | Rows affected by the first statement |
| `total_duration` | number | Total execution time in seconds |
| `result_count` | number | Number of results (for multi-statement queries) |

### Example Usage

```jinja
{%- set output = doc.named['user_query'].output %}

{# Check if we got results #}
{% if output.total_rows > 0 %}
  Found {{ output.total_rows }} users:
  {% for row in output.rows %}
  - {{ row.name }} ({{ row.email }})
  {% endfor %}
{% else %}
  No users found.
{% endif %}
```

### Multi-Statement Queries

SQL blocks support multiple statements separated by semicolons. Each statement produces a separate result:

```sql
SELECT COUNT(*) as total FROM users;
SELECT * FROM users LIMIT 5;
```

Access individual results via the `results` array:

```jinja
{%- set output = doc.named['multi_query'].output %}
Total users: {{ output.results[0].data.rows[0].total }}
First 5 users: {{ output.results[1].data.rows }}
```
