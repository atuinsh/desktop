CREATE TABLE
  context (document_id TEXT, block_id TEXT, context TEXT);

CREATE INDEX idx_context_document_id ON context (document_id);

CREATE INDEX idx_context_block_id ON context (block_id);

CREATE UNIQUE INDEX idx_context_document_id_block_id ON context (document_id, block_id);
