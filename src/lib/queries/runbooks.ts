import { queryOptions } from "@tanstack/react-query";
import { localQuery } from "./local_query";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import { getRunbookID, HttpResponseError } from "@/api/api";
import AtuinEnv from "@/atuin_env";
import { RemoteRunbook } from "@/state/models";

export function runbookById(id: string | undefined) {
  return queryOptions({
    ...localQuery,
    retry: 10,
    retryDelay: 500, // gives the workspace manager up to 5 seconds to handle loading FS runbooks metadata
    queryKey: ["runbook", id],
    queryFn: () => {
      if (id) {
        return Runbook.load(id);
      } else {
        return Promise.resolve(null);
      }
    },
  });
}

export function runbooksByWorkspaceId(workspaceId: string | undefined) {
  return queryOptions({
    ...localQuery,
    queryKey: ["runbooks", "workspace", workspaceId],
    queryFn: () => {
      if (workspaceId) {
        return Runbook.allFromWorkspace(workspaceId);
      } else {
        return Promise.resolve([]);
      }
    },
  });
}

export function runbooksByLegacyWorkspaceId(legacyWorkspaceId: string | undefined) {
  return queryOptions({
    ...localQuery,
    queryKey: ["runbooks", "legacy_workspace", legacyWorkspaceId],
    queryFn: () => {
      if (legacyWorkspaceId) {
        // Only online runbooks can have a legacy workspace ID
        return OnlineRunbook.all(legacyWorkspaceId);
      } else {
        return Promise.resolve([]);
      }
    },
  });
}

export function allRunbooks() {
  return queryOptions({
    ...localQuery,
    queryKey: ["runbooks", "all"],
    queryFn: () => Runbook.allInAllWorkspaces(),
  });
}

export function allRunbookIds() {
  return queryOptions({
    ...localQuery,
    queryKey: ["runbooks", "all-ids"],
    queryFn: () => Runbook.allIdsInAllWorkspaces(),
  });
}

export function remoteRunbook(runbookOrId?: Runbook | string) {
  return queryOptions<RemoteRunbook | null>({
    staleTime: 1000 * 30,
    queryKey: ["remote_runbook", typeof runbookOrId === "string" ? runbookOrId : runbookOrId?.id],
    queryFn: async () => {
      if (typeof runbookOrId === "string") {
        const rb = await getRunbookID(runbookOrId);
        return rb;
      } else if (runbookOrId) {
        try {
          const rb = await getRunbookID(runbookOrId.id);
          return rb;
        } catch (err: any) {
          if (
            (err instanceof HttpResponseError &&
              err.code === 404 &&
              // Only clear out the cache on 404 if the runbook is from our environment or was created locally
              runbookOrId.source === AtuinEnv.hubRunbookSource) ||
            runbookOrId.source === "local"
          ) {
            return null;
          } else {
            throw err;
          }
        }
      } else {
        throw new Error("no runbook ID specified");
      }
    },
    initialData:
      typeof runbookOrId !== "string" &&
      runbookOrId?.isOnline() &&
      (runbookOrId as OnlineRunbook).remoteInfo
        ? (JSON.parse((runbookOrId as OnlineRunbook).remoteInfo!) as RemoteRunbook)
        : undefined,
    refetchOnMount: "always",
  });
}
