CREATE TABLE
  saved_blocks (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    created BIGINT NOT NULL,
    updated BIGINT NOT NULL
  );
