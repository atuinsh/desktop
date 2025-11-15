-- Add up migration script here
alter table exec_log rename column exit to output;