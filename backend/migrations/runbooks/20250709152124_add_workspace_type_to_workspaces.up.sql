ALTER TABLE workspaces
ADD COLUMN online INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_workspaces_type ON workspaces (online);
