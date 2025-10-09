UPDATE workspaces
SET
  online = 1
WHERE
  org_id IS NOT NULL;
