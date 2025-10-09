---
description: Connect to ClickHouse databases for real-time analytics queries and time-series analysis over large datasets via HTTP interface
---

# :simple-clickhouse: ClickHouse

The ClickHouse block allows you to connect to ClickHouse databases and execute queries directly within your runbook.

## Connection

Configure your ClickHouse connection using a standard ClickHouse HTTP URL:

```
http://username:password@host:8123/database
```

!!! info "HTTP Protocol Only"
    Currently, Atuin Desktop uses the ClickHouse HTTP interface (typically port 8123) and only supports connection via URL format. Individual connection parameters and the native TCP protocol are not supported at this time.

## Query Execution

Write and execute SQL queries against your ClickHouse database. Results are displayed in a table format within the runbook. ClickHouse excels at:

- Real-time analytics queries
- Time-series data analysis
- Aggregation over large datasets
- Column-oriented data processing
- Complex analytical functions

!!! info "Performance"
    ClickHouse is optimized for analytical workloads and can handle queries over billions of rows efficiently.

## Template Usage

All input fields are first rendered by the [templating](../../templating.md) system, allowing you to use variables in your queries and connection parameters.

```sql
SELECT 
    toStartOfDay(timestamp) as date,
    count() as events
FROM events 
WHERE timestamp >= '{{var.start_date}}'
  AND event_type = '{{var.event_type}}'
GROUP BY date
ORDER BY date;
```

!!! example "Time-Series Queries"
    ClickHouse excels at time-series analysis. Use template variables to create flexible date ranges and filtering conditions.

## Security

Consider using [secrets](../../secrets.md) for sensitive connection parameters like passwords to avoid storing credentials in plain text.

!!! warning "Security Best Practices"
    - Never hardcode database passwords in runbooks
    - Use read-only users for analytics queries
    - Consider IP whitelisting for production ClickHouse instances
    - Be mindful of query resource usage on shared clusters
