use sqlx::{sqlite::SqliteRow, FromRow, Row, SqlitePool};
use uuid::Uuid;

use atuin_desktop_runtime::context::{BlockContext, BlockContextStorage};

struct BlockContextWrapper(BlockContext);

pub struct SqliteContextStorage {
    pool: SqlitePool,
}

impl SqliteContextStorage {
    pub async fn new(pool: SqlitePool) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl BlockContextStorage for SqliteContextStorage {
    async fn save(
        &self,
        document_id: &str,
        block_id: &Uuid,
        context: &BlockContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query(
            "INSERT INTO context (document_id, block_id, context) VALUES (?, ?, ?) \
                ON CONFLICT(document_id, block_id) DO UPDATE SET context = ?",
        )
        .bind(document_id)
        .bind(block_id.to_string())
        .bind(serde_json::to_string(context)?)
        .bind(serde_json::to_string(context)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn load(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let context: Option<BlockContextWrapper> =
            sqlx::query_as("SELECT context FROM context WHERE document_id = ? AND block_id = ?")
                .bind(document_id)
                .bind(block_id.to_string())
                .fetch_optional(&self.pool)
                .await?;

        Ok(context.map(|c| c.0))
    }

    async fn delete(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query("DELETE FROM context WHERE document_id = ? AND block_id = ?")
            .bind(document_id)
            .bind(block_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_for_document(
        &self,
        document_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query("DELETE FROM context WHERE document_id = ?")
            .bind(document_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

impl<'a> FromRow<'a, SqliteRow> for BlockContextWrapper {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        let json_context = row.get::<String, _>("context");
        let context: BlockContext =
            serde_json::from_str(&json_context).map_err(|e| sqlx::Error::ColumnDecode {
                index: "context".to_string(),
                source: Box::new(e),
            })?;
        Ok(BlockContextWrapper(context))
    }
}
