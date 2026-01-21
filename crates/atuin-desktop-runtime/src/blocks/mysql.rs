pub mod decode;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, MySqlDialect};
use sqlx::{mysql::MySqlConnectOptions, Column, MySqlPool, Row};
use std::str::FromStr;
use std::time::Instant;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{
    Block, BlockBehavior, FromDocument, QueryBlockBehavior, SqlBlockBehavior, SqlBlockError,
    SqlBlockExecutionResult, SqlQueryResult, SqlStatementResult,
};
use crate::execution::{ExecutionContext, ExecutionHandle};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Mysql {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub query: String,

    #[builder(setter(into))]
    pub uri: String,

    #[builder(default = 0)]
    pub auto_refresh: u32,

    #[builder(default = false)]
    pub skip_sql_mode_init: bool,
}

impl FromDocument for Mysql {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Block has no props")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        let mysql = Mysql::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MySQL Query")
                    .to_string(),
            )
            .query(
                props
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .uri(
                props
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .auto_refresh(
                props
                    .get("autoRefresh")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
            .skip_sql_mode_init(
                props
                    .get("skipSqlModeInit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
            .build();

        Ok(mysql)
    }
}

impl Mysql {
    /// Validate MySQL URI format and connection parameters
    fn validate_mysql_uri(uri: &str) -> Result<(), String> {
        if uri.is_empty() {
            return Err("MySQL URI cannot be empty".to_string());
        }

        if !uri.starts_with("mysql://") && !uri.starts_with("mariadb://") {
            return Err(
                "Invalid MySQL URI format. Must start with 'mysql://' or 'mariadb://'".to_string(),
            );
        }

        // Try parsing the URI to catch format errors early
        if let Err(e) = MySqlConnectOptions::from_str(uri) {
            return Err(format!("Invalid URI format: {}", e));
        }

        Ok(())
    }

    /// Convert MySQL row to JSON value using existing decode module
    fn row_to_json(row: &sqlx::mysql::MySqlRow) -> Result<Map<String, Value>, sqlx::Error> {
        let mut obj = Map::new();

        for (i, column) in row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let raw_value = row.try_get_raw(i)?;

            // Use existing MySQL decode function
            let value = decode::to_json(raw_value).unwrap_or(Value::Null);

            obj.insert(column_name, value);
        }

        Ok(obj)
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Mysql {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Mysql(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        QueryBlockBehavior::execute_query_block(self, context).await
    }
}

#[async_trait::async_trait]
impl SqlBlockBehavior for Mysql {
    type Pool = MySqlPool;

    fn dialect() -> Box<dyn Dialect> {
        Box::new(MySqlDialect {})
    }

    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, SqlBlockError> {
        context
            .context_resolver
            .resolve_template(&self.query)
            .map_err(|e| SqlBlockError::InvalidTemplate(e.to_string()))
    }

    fn resolve_uri(&self, context: &ExecutionContext) -> Result<String, SqlBlockError> {
        let uri = context
            .context_resolver
            .resolve_template(&self.uri)
            .map_err(|e| SqlBlockError::InvalidTemplate(e.to_string()))?;

        if let Err(e) = Self::validate_mysql_uri(&uri) {
            return Err(SqlBlockError::InvalidUri(e.to_string()));
        }

        Ok(uri)
    }

    async fn create_pool(&self, uri: String) -> Result<Self::Pool, SqlBlockError> {
        let mut opts = MySqlConnectOptions::from_str(&uri)?;

        // Skip sql_mode initialization for databases that don't support dynamic SET statements
        // (e.g., StarRocks, Apache Doris)
        if self.skip_sql_mode_init {
            opts = opts.pipes_as_concat(false).no_engine_substitution(false);
        }

        Ok(MySqlPool::connect_with(opts).await?)
    }

    async fn close_pool(&self, pool: &Self::Pool) -> Result<(), SqlBlockError> {
        pool.close().await;
        Ok(())
    }

    fn is_query(statement: &Statement) -> bool {
        matches!(
            statement,
            Statement::Query { .. }
                | Statement::Explain { .. }
                | Statement::ExplainTable { .. }
                | Statement::Fetch { .. }
                | Statement::Pragma { .. }
                | Statement::ShowVariables { .. }
                | Statement::ShowCreate { .. }
                | Statement::ShowColumns { .. }
                | Statement::ShowTables { .. }
                | Statement::ShowCollation { .. }
        )
    }

    async fn execute_sql_query(
        &self,
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError> {
        let start_time = Instant::now();
        let rows = sqlx::query(query).fetch_all(pool).await?;
        let duration = start_time.elapsed();
        let mut columns = Vec::new();

        if let Some(first_row) = rows.first() {
            columns = first_row
                .columns()
                .iter()
                .map(|col| col.name().to_string())
                .collect();
        }

        let results = rows
            .iter()
            .map(Self::row_to_json)
            .collect::<Result<_, _>>()?;

        Ok(SqlBlockExecutionResult::Query(
            SqlQueryResult::builder()
                .columns(columns)
                .rows(results)
                .duration(duration)
                .build(),
        ))
    }

    async fn execute_sql_statement(
        &self,
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError> {
        let start_time = Instant::now();
        let result = sqlx::query(statement).execute(pool).await?;
        let duration = start_time.elapsed();

        Ok(SqlBlockExecutionResult::Statement(
            SqlStatementResult::builder()
                .rows_affected(result.rows_affected())
                .duration(duration)
                .build(),
        ))
    }
}
