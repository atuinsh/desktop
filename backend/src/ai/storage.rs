use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row, SqlitePool};
use uuid::Uuid;

use crate::ai::fsm;
use crate::ai::types::ModelSelection;

/// Inner struct for JSON serialization (excludes runbook_id and updated_at which are columns)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionData {
    pub model: ModelSelection,
    pub agent_state: fsm::State,
    pub agent_context: fsm::Context,
}

#[derive(Debug, Clone)]
pub struct SerializedAISession {
    pub id: Uuid,
    pub runbook_id: Uuid,
    pub model: ModelSelection,
    pub agent_state: fsm::State,
    pub agent_context: fsm::Context,
    pub updated_at: i64,
}

impl FromRow<'_, SqliteRow> for SerializedAISession {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        let session_json: String = row.get("session");
        let data: SessionData =
            serde_json::from_str(&session_json).map_err(|e| sqlx::Error::ColumnDecode {
                index: "session".to_string(),
                source: Box::new(e),
            })?;

        let id_str: String = row.get("id");
        let id: Uuid = serde_json::from_str(&id_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "id".to_string(),
            source: Box::new(e),
        })?;

        let runbook_id_str: String = row.get("runbook_id");
        let runbook_id: Uuid =
            serde_json::from_str(&runbook_id_str).map_err(|e| sqlx::Error::ColumnDecode {
                index: "runbook_id".to_string(),
                source: Box::new(e),
            })?;

        Ok(SerializedAISession {
            id,
            runbook_id,
            model: data.model,
            agent_state: data.agent_state,
            agent_context: data.agent_context,
            updated_at: row.get("updated_at"),
        })
    }
}

pub struct AISessionStorage {
    pool: SqlitePool,
}

impl AISessionStorage {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Save or update a session
    pub async fn save(
        &self,
        session: &SerializedAISession,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = SessionData {
            model: session.model.clone(),
            agent_state: session.agent_state.clone(),
            agent_context: session.agent_context.clone(),
        };

        sqlx::query(
            "INSERT OR REPLACE INTO ai_sessions (id, runbook_id, session, updated_at) VALUES (?, ?, ?, ?)"
        )
            .bind(serde_json::to_string(&session.id)?)
            .bind(serde_json::to_string(&session.runbook_id)?)
            .bind(serde_json::to_string(&data)?)
            .bind(session.updated_at)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Find the most recent session for a runbook
    pub async fn find_most_recent_for_runbook(
        &self,
        runbook_id: &Uuid,
    ) -> Result<Option<SerializedAISession>, Box<dyn std::error::Error + Send + Sync>> {
        let session: Option<SerializedAISession> = sqlx::query_as(
            "SELECT * FROM ai_sessions WHERE runbook_id = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(serde_json::to_string(runbook_id)?)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }
}
