# Database Migrations

This document describes how to create a new database and add migrations using the `DbInstances` system.

## Overview

The application uses SQLite databases with SQLx for migrations. The `DbInstances` struct in `backend/src/sqlite.rs` manages multiple database connections and automatically runs migrations when a pool is first accessed.

## Creating a New Database

### 1. Create Migration Directory

Create a new directory for your database migrations:

```bash
mkdir -p backend/migrations/<database-name>
```

### 2. Create Migration Files

Use the SQLx CLI to create new migration files:

```bash
sqlx migrate add --source backend/migrations/<database-name> -r <migration_name>
```

The `-r` flag creates reversible migrations (with both `up` and `down` files).

This will create two files:
- `backend/migrations/<database-name>/<timestamp>_<migration_name>.up.sql` - Contains the migration
- `backend/migrations/<database-name>/<timestamp>_<migration_name>.down.sql` - Contains the rollback

Example:
```bash
sqlx migrate add --source backend/migrations/context -r initial_schema
```

Each migration file should contain SQL statements for schema changes.

### 3. Register the Migrator

In `backend/src/state.rs`, add your migrator during the `init` function:

```rust
self.db_instances.init().await?;
self.db_instances
    .add_migrator("<database-name>", sqlx::migrate!("./migrations/<database-name>"))
    .await?;
```

Example from the codebase:

```rust
self.db_instances
    .add_migrator("context", sqlx::migrate!("./migrations/context"))
    .await?;
```

### 4. Access the Database

Get a connection pool to your database:

```rust
let pool = state.db_instances.get_pool("<database-name>").await?;
```

The first time `get_pool` is called for a database:
1. The database file is created if it doesn't exist
2. A connection pool is established
3. Any registered migrations are automatically run
4. The pool is cached for subsequent calls

## How It Works

### DbInstances Structure

The `DbInstances` struct manages:
- Multiple SQLite database connections
- Migration runners for each database
- Automatic migration execution
- Development/production database separation via prefixes

### Key Methods

#### `add_migrator(database: &str, migrator: Migrator)`
Registers a migrator for a specific database. Must be called before the first `get_pool` call for that database.

#### `get_pool(database: &str)`
Returns a connection pool for the specified database:
- Creates the database if it doesn't exist
- Configures SQLite with WAL mode and optimizations
- Runs migrations on first access
- Caches and returns the pool for subsequent calls

#### `init()`
Creates the application data directory if it doesn't exist. Should be called once during application startup.

#### `shutdown()`
Closes all connection pools and clears the cache.

## Database Naming

Database names are automatically normalized:
- If you pass `"mydb.db"`, the name becomes `"mydb"`
- If you pass `"mydb"`, the file becomes `"mydb.db"`

In development mode (with `dev_prefix`), databases are prefixed:
- Production: `mydb.db`
- Development: `dev_mydb.db`

## SQLite Configuration

Databases are configured with:
- **Journal Mode**: WAL (Write-Ahead Logging)
- **Synchronous**: Normal
- **Optimize on Close**: Enabled
- **Regex Support**: Enabled
- **Create if Missing**: Enabled
- **Connection Timeout**: 3 seconds
