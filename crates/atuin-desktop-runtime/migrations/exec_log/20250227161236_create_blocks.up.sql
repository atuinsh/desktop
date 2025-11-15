-- Map an integer block id to a UUID
-- SQLite does not support native UUIDs, so we map them like this
-- Might also make sense to extend this table to include other metadata and stats in the future
CREATE TABLE blocks(id integer PRIMARY KEY, uuid TEXT UNIQUE);