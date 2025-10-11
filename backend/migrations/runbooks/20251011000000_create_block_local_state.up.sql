CREATE TABLE
  block_local_state (
    runbook_id TEXT NOT NULL,
    block_id TEXT NOT NULL,
    property_name TEXT NOT NULL,
    property_value TEXT NOT NULL,
    created BIGINT NOT NULL,
    updated BIGINT NOT NULL,
    PRIMARY KEY (runbook_id, block_id, property_name)
  );

CREATE INDEX idx_block_local_state_runbook ON block_local_state (runbook_id);
CREATE INDEX idx_block_local_state_block ON block_local_state (runbook_id, block_id);

