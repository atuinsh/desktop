import { queryOptions } from "@tanstack/react-query";
import { localQuery } from "./local_query";
import LegacyWorkspace from "@/state/runbooks/legacy_workspace";
import Workspace from "@/state/runbooks/workspace";
import WorkspaceManager from "../workspaces/manager";

export function allWorkspaces() {
  return queryOptions({
    ...localQuery,
    queryKey: ["workspaces"],
    queryFn: async () => {
      return await Workspace.all();
    },
  });
}

export function allLegacyWorkspaces() {
  return queryOptions({
    ...localQuery,
    queryKey: ["legacy_workspaces"],
    queryFn: async () => {
      const wss = await LegacyWorkspace.all();
      const promises = wss.map((ws) => ws.refreshMeta());
      await Promise.all(promises);
      return wss;
    },
  });
}

export function workspaceById(id: string | null) {
  return queryOptions({
    ...localQuery,
    queryKey: ["workspace", id],
    queryFn: async () => {
      if (!id) return null;
      return await Workspace.get(id);
    },
  });
}

export function legacyWorkspaceById(id: string) {
  return queryOptions({
    ...localQuery,
    queryKey: ["legacy_workspace", id],
    queryFn: async () => {
      const ws = await LegacyWorkspace.findById(id);
      if (ws) {
        await ws.refreshMeta();
      }
      return ws;
    },
  });
}

export function userOwnedWorkspaces() {
  return orgWorkspaces(null);
}

export function orgWorkspaces(orgId: string | null) {
  return queryOptions({
    ...localQuery,
    queryKey: ["workspaces", "org", orgId],
    queryFn: async () => {
      return await Workspace.all({ orgId });
    },
  });
}

export function localWorkspaceInfo(workspaceId: string) {
  const manager = WorkspaceManager.getInstance();
  const info = manager.getWorkspaceInfo(workspaceId);

  return queryOptions({
    ...localQuery,
    queryKey: ["workspace_info", workspaceId],
    queryFn: async () => {
      const manager = WorkspaceManager.getInstance();
      const info = manager.getWorkspaceInfo(workspaceId);
      return info;
    },
    initialData: info,
  });
}
