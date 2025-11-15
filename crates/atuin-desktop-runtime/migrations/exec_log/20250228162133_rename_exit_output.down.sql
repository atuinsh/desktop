-- Add down migration script here
alter table exec_log rename column output to exit;