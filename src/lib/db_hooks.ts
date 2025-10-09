import LegacyWorkspace from "@/state/runbooks/legacy_workspace";
import {
  allRunbookIds,
  allRunbooks,
  remoteRunbook,
  runbookById,
  runbooksByLegacyWorkspaceId,
  runbooksByWorkspaceId,
} from "./queries/runbooks";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";
import { InvalidateQueryFilters } from "@tanstack/react-query";
import {
  allLegacyWorkspaces,
  allWorkspaces,
  legacyWorkspaceById,
  orgWorkspaces,
  userOwnedWorkspaces,
  workspaceById,
} from "./queries/workspaces";
import Snapshot from "@/state/runbooks/snapshot";
import { snapshotByRunbookAndTag, snapshotsByRunbook } from "./queries/snapshots";
import Operation from "@/state/runbooks/operation";
import Workspace from "@/state/runbooks/workspace";

export type Model = "runbook" | "workspace" | "legacy_workspace" | "snapshot";
export type Action = "create" | "update" | "delete";

export async function dbHook(kind: Model, action: Action, model: any) {
  invalidateQueryKeys(kind, action, model);

  // TODO: these should only be fired by user action, not by server events
  // So we'll need to move this out of db hooks
  if (kind === "runbook" && action === "delete") {
    model = model as Runbook;
    const workspace = await Workspace.get(model.workspaceId);
    if (workspace && workspace.isOnline()) {
      const op = new Operation({ operation: { type: "runbook_deleted", runbookId: model.id } });
      op.save();
    }
  }

  if (kind === "workspace" && action === "delete") {
    model = model as Workspace;
    if (model.isOnline()) {
      const op = new Operation({
        operation: { type: "workspace_deleted", workspaceId: model.get("id") },
      });
      op.save();
    }

    // This, however, should remain in the db hook
    const runbooks = await Runbook.allFromWorkspace(model.get("id")!);
    const promises = runbooks.map(async (runbook) => {
      // post-delete runbook operations will delete remote runbook
      await runbook.delete();
    });
    await Promise.allSettled(promises);
  }
}

export function invalidateQueryKeys(kind: Model, action: Action, model: any) {
  const queryKeys = getQueryKeys(kind, action, model);

  queryKeys.forEach((queryKey) => {
    if (Array.isArray(queryKey)) {
      queryKey = { queryKey };
    }

    useStore.getState().queryClient.invalidateQueries(queryKey);
  });
}

function getQueryKeys(kind: Model, action: Action, model: any): InvalidateQueryFilters<any>[] {
  switch (kind) {
    case "runbook":
      return getRunbookQueryKeys(action, model as Runbook);
    case "legacy_workspace":
      return getLegacyWorkspaceQueryKeys(action, model as LegacyWorkspace);
    case "workspace":
      return getWorkspaceQueryKeys(action, model as Workspace);
    case "snapshot":
      return getSnapshotQueryKeys(action, model as Snapshot);
  }
}

function getRunbookQueryKeys(action: Action, model: Runbook): InvalidateQueryFilters<any>[] {
  let keys = [];

  switch (action) {
    case "create":
      keys = [
        allRunbookIds(),
        allRunbooks(),
        allLegacyWorkspaces(),
        runbooksByWorkspaceId(model.workspaceId),
        remoteRunbook(model),
        runbookById(model.id),
      ];
      break;
    case "update":
      keys = [
        allRunbooks(),
        runbookById(model.id),
        runbooksByWorkspaceId(model.workspaceId),
        remoteRunbook(model),
      ];
      break;
    case "delete":
      keys = [
        allRunbookIds(),
        allRunbooks(),
        runbookById(model.id),
        allLegacyWorkspaces(),
        runbooksByWorkspaceId(model.workspaceId),
      ];
      break;
  }

  if (model instanceof OnlineRunbook) {
    keys.push(runbooksByLegacyWorkspaceId(model.legacyWorkspaceId));
  }

  return keys;
}

function getLegacyWorkspaceQueryKeys(
  action: Action,
  model: LegacyWorkspace,
): InvalidateQueryFilters<any>[] {
  switch (action) {
    case "create":
      return [allLegacyWorkspaces()];
    case "update":
      return [allLegacyWorkspaces(), legacyWorkspaceById(model.id)];
    case "delete":
      return [allLegacyWorkspaces()];
  }
}

function getWorkspaceQueryKeys(action: Action, model: Workspace): InvalidateQueryFilters<any>[] {
  const isOrgWorkspace = !!model.get("orgId");

  switch (action) {
    case "create":
      if (isOrgWorkspace) {
        return [allWorkspaces(), orgWorkspaces(model.get("orgId")!)];
      } else {
        return [allWorkspaces(), userOwnedWorkspaces()];
      }
    case "update":
      return [
        allWorkspaces(),
        userOwnedWorkspaces(),
        workspaceById(model.get("id")!),
        orgWorkspaces(model.get("orgId")!),
      ];
    case "delete":
      if (isOrgWorkspace) {
        return [allWorkspaces(), orgWorkspaces(model.get("orgId")!)];
      } else {
        return [allWorkspaces(), userOwnedWorkspaces()];
      }
  }
}

function getSnapshotQueryKeys(action: Action, model: Snapshot): InvalidateQueryFilters<any>[] {
  switch (action) {
    case "create":
      return [snapshotsByRunbook(model.runbook_id)];
    case "update":
      return [
        snapshotsByRunbook(model.runbook_id),
        snapshotByRunbookAndTag(model.runbook_id, model.tag),
      ];
    case "delete":
      return [
        snapshotsByRunbook(model.runbook_id),
        snapshotByRunbookAndTag(model.runbook_id, model.tag),
      ];
  }
}
