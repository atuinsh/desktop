CREATE TABLE ai_sessions (
  id TEXT PRIMARY KEY,
  session TEXT NOT NULL,
  runbook_id TEXT NOT NULL,
  updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_ai_sessions_runbook ON ai_sessions(runbook_id, updated_at DESC);
