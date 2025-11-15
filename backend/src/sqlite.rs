use std::{collections::HashMap, path::PathBuf, str::FromStr, time::Duration};

use eyre::Result;
use sqlx::{migrate::Migrator, sqlite, SqlitePool};
use tokio::{fs::create_dir_all, runtime::Handle, sync::Mutex};

pub struct DbInstances {
    app_path: PathBuf,
    dev_prefix: Option<String>,
    instances: Mutex<HashMap<String, SqlitePool>>,
    migrations: Mutex<HashMap<String, Migrator>>,
}

impl DbInstances {
    pub fn new(app_path: PathBuf, dev_prefix: Option<String>) -> Self {
        Self {
            app_path,
            dev_prefix,
            instances: Mutex::new(Default::default()),
            migrations: Mutex::new(Default::default()),
        }
    }

    /// Adds a migrator for a database. The migrator will automatically run when the pool is first accessed via `get_pool`.
    pub async fn add_migrator(&self, database: &str, migrator: Migrator) -> Result<()> {
        let mut migrations = self.migrations.lock().await;
        migrations.insert(database.to_string(), migrator);
        Ok(())
    }

    pub async fn init(&self) -> Result<()> {
        create_dir_all(&self.app_path).await?;

        Ok(())
    }

    /// Gets a pool for a database. If the database does not exist, it will be created.
    /// The migrator for the database, if any, will be run on the pool the first time it is accessed.
    pub async fn get_pool(&self, database: &str) -> Result<SqlitePool> {
        let db_name = self.db_name(database);
        let db_file = self.db_file(database);
        let mut instances = self.instances.lock().await;

        let pool = instances.get(&db_file).cloned();
        if let Some(pool) = pool {
            return Ok(pool);
        }

        let opts = sqlite::SqliteConnectOptions::from_str(&self.conn_url(&db_file))?
            .journal_mode(sqlite::SqliteJournalMode::Wal)
            .optimize_on_close(true, None)
            .synchronous(sqlite::SqliteSynchronous::Normal)
            .with_regexp()
            .create_if_missing(true);

        let pool = sqlite::SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs_f64(3.0))
            .connect_with(opts)
            .await?;

        if let Some(migrator) = self.migrations.lock().await.get(&db_name) {
            migrator.run(&pool).await?;
        }

        instances.insert(db_file, pool.clone());
        Ok(pool)
    }

    /// Closes all pools and clears the instances map.
    pub async fn shutdown(&self) -> Result<()> {
        let mut instances = self.instances.lock().await;
        for pool in instances.values() {
            pool.close().await;
        }
        instances.clear();
        Ok(())
    }

    fn conn_url(&self, db_file: &str) -> String {
        format!("sqlite:{}", self.app_path.join(db_file).to_str().unwrap())
    }

    fn db_name(&self, database: &str) -> String {
        if database.ends_with(".db") {
            database.trim_end_matches(".db").to_string()
        } else {
            database.to_string()
        }
    }

    fn db_file(&self, database: &str) -> String {
        let ext = if database.ends_with(".db") { "" } else { ".db" };

        self.dev_prefix
            .as_ref()
            .map_or(format!("{database}{ext}"), |prefix| {
                format!("{prefix}_{database}{ext}")
            })
    }
}

impl Drop for DbInstances {
    fn drop(&mut self) {
        let _ = Handle::current().block_on(self.shutdown());
    }
}
