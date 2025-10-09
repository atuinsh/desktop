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
