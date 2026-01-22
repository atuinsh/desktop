use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row, SqlitePool};
use uuid::Uuid;

use crate::ai::fsm;
use crate::ai::types::{ChargeTarget, ModelSelection, SessionConfig, SessionKind};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

/// The original version of the serialization format.
/// It has a few quirks:
///
/// - `id` and `runbook_id` were mistakenly stored as JSON strings
/// - `updated_at` was not stored in the session data
///
/// We take care of these during DB loading and conversion to the V1 format, below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedAISessionV0 {
    #[serde(skip)]
    pub id: Uuid,
    #[serde(skip)]
    pub runbook_id: Uuid,
    pub model: ModelSelection,
    pub agent_state: fsm::State,
    pub agent_context: fsm::Context,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedAISessionV1 {
    pub id: Uuid,
    pub config: SessionConfig,
    pub kind: SessionKind,
    pub agent_state: fsm::State,
    pub agent_context: fsm::Context,
}

impl From<SerializedAISessionV0> for SerializedAISessionV1 {
    fn from(session: SerializedAISessionV0) -> Self {
        Self {
            id: session.id,
            config: SessionConfig {
                model: session.model,
                desktop_username: "".to_string(),
                charge_target: ChargeTarget::User,
            },
            kind: SessionKind::AssistantChat {
                runbook_id: session.runbook_id,
                block_infos: vec![],
            },
            agent_state: session.agent_state,
            agent_context: session.agent_context,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version", content = "data")]
pub enum SerializedAISession {
    V0(SerializedAISessionV0),
    V1(SerializedAISessionV1),
}

impl SerializedAISession {
    pub fn to_latest(&self) -> SerializedAISessionV1 {
        match self {
            SerializedAISession::V0(session) => SerializedAISessionV1::from(session.clone()),
            SerializedAISession::V1(session) => session.clone(),
        }
    }
}

impl FromRow<'_, SqliteRow> for SerializedAISession {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        let version: i64 = row.get("version");

        match version {
            0 => {
                // Version 0 of the serialization format mistakenly stored the id and runbook_id as JSON.
                let id = json_str_to_uuid(&row.get::<String, _>("id"))?;
                let runbook_id = json_str_to_uuid(&row.get::<String, _>("runbook_id"))?;

                let session_json: String = row.get("session");
                let mut session: SerializedAISessionV0 = serde_json::from_str(&session_json)
                    .map_err(|e| sqlx::Error::ColumnDecode {
                        index: "session".to_string(),
                        source: Box::new(e),
                    })?;
                session.id = id;
                session.runbook_id = runbook_id;
                Ok(SerializedAISession::V0(session))
            }
            1 => {
                let session_json: String = row.get("session");
                let session: SerializedAISessionV1 =
                    serde_json::from_str(&session_json).map_err(|e| sqlx::Error::ColumnDecode {
                        index: "session".to_string(),
                        source: Box::new(e),
                    })?;
                Ok(SerializedAISession::V1(session))
            }
            _ => Err(sqlx::Error::ColumnDecode {
                index: "version".to_string(),
                source: Box::new(std::io::Error::other("Invalid version")),
            }),
        }
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
        runbook_id: &Uuid,
        session: &SerializedAISessionV1,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT OR REPLACE INTO ai_sessions (version, id, runbook_id, session, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
            .bind(1)
            .bind(session.id.to_string())
            .bind(runbook_id.to_string())
            .bind(serde_json::to_string(&session)?)
            .bind(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Find the most recent session for a runbook
    pub async fn find_most_recent_for_runbook(
        &self,
        runbook_id: &Uuid,
    ) -> Result<Option<SerializedAISessionV1>, StorageError> {
        // Version 0 of the serialization format mistakenly stored the runbook_id as a JSON string.
        let session: Option<SerializedAISession> = sqlx::query_as(
            "SELECT * FROM ai_sessions WHERE runbook_id = ? OR runbook_id = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(serde_json::to_string(runbook_id)?)
        .bind(runbook_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(session.map(|s| s.to_latest()))
    }
}

fn json_str_to_uuid(json_str: &str) -> Result<Uuid, sqlx::Error> {
    let id_str =
        serde_json::from_str::<String>(json_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "id".to_string(),
            source: Box::new(e),
        })?;
    Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::ColumnDecode {
        index: "id".to_string(),
        source: Box::new(e),
    })
}
