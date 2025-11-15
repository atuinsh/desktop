-- Store the actual log of all block executions
-- Currently just execution ID, block ID, timestamp and exit. Will absolutely be extended to include more
-- information in the future.
-- Using text for the exit code to allow for arbitrary types, as it isn't necessarily a shell exit code
CREATE TABLE exec_log(
    id integer PRIMARY KEY, 
    block_id integer not null, 
    start_time integer not null, 
    end_time integer not null, 
    exit text not null,
    FOREIGN KEY (block_id) REFERENCES blocks(id)
);