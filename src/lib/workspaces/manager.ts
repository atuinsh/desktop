import Workspace from "@/state/runbooks/workspace";
import { SharedStateManager } from "../shared_state/manager";
import { AtuinSharedStateAdapter } from "../shared_state/adapter";
import type { WorkspaceState } from "@rust/WorkspaceState";
import type { WorkspaceError } from "@rust/WorkspaceError";
import { watchWorkspace, unwatchWorkspace } from "./commands";
import { localWorkspaceInfo } from "../queries/workspaces";
import { useStore } from "@/state/store";
import { allRunbookIds, allRunbooks, runbookById } from "../queries/runbooks";
import Emittery from "emittery";
import { OfflineRunbook } from "@/state/runbooks/runbook";

export default class WorkspaceManager {
  private static instance: WorkspaceManager;
  private workspaces: Map<string, Result<WorkspaceState, WorkspaceError>> = new Map();
  private emitter: Emittery;

  private constructor() {
    this.emitter = new Emittery();
  }

  public static getInstance(): WorkspaceManager {
    if (!WorkspaceManager.instance) {
      WorkspaceManager.instance = new WorkspaceManager();
    }
    return WorkspaceManager.instance;
  }

  public async watchWorkspace(workspace: Workspace) {
    if (workspace.isOnline() || workspace.isLegacyHybrid()) {
      SharedStateManager.getInstance(
        `workspace-folder:${workspace.get("id")}`,
        new AtuinSharedStateAdapter(`workspace-folder:${workspace.get("id")}`),
      );
    } else {
      if (this.workspaces.has(workspace.get("id")!)) {
        await this.unwatchWorkspace(workspace);
      }

      await watchWorkspace(workspace.get("folder")!, workspace.get("id")!, async (event) => {
        const queryClient = useStore.getState().queryClient;
        switch (event.type) {
          case "State":
            this.workspaces.set(workspace.get("id")!, Ok(event.data));
            if (workspace.get("name") !== event.data.name) {
              workspace.set("name", event.data.name);
              await workspace.save();
            }
            queryClient.invalidateQueries(localWorkspaceInfo(workspace.get("id")!));
            queryClient.invalidateQueries(allRunbookIds());
            queryClient.invalidateQueries(allRunbooks());
            break;
          case "Error":
            this.workspaces.set(workspace.get("id")!, Err(event.data));
            queryClient.invalidateQueries(localWorkspaceInfo(workspace.get("id")!));
            queryClient.invalidateQueries(allRunbookIds());
            queryClient.invalidateQueries(allRunbooks());
            break;
          case "RunbookChanged":
            console.log("runbook changed", event.data);
            useStore.getState().queryClient.invalidateQueries(runbookById(event.data));
            const runbook = await OfflineRunbook.load(event.data);
            this.emitter.emit("runbook-changed", runbook);
            break;
          case "RunbookDeleted":
            this.emitter.emit("runbook-deleted", event.data);
            break;
          default:
            const exhaustiveCheck: never = event;
            throw new Error(`Unhandled workspace event: ${exhaustiveCheck}`);
        }
      });
    }
  }

  public onRunbookDeleted(callback: (runbookId: string) => void) {
    return this.emitter.on("runbook-deleted", callback);
  }

  public onRunbookChanged(callback: (runbook: OfflineRunbook, contentHash: string) => void) {
    return this.emitter.on("runbook-changed", (runbook) => {
      callback(runbook, runbook.contentHash);
    });
  }

  public async unwatchWorkspace(workspace: Workspace) {
    if (workspace.isOnline() || workspace.isLegacyHybrid()) {
      SharedStateManager.stopInstance(`workspace-folder:${workspace.get("id")}`);
    } else {
      if (!this.workspaces.has(workspace.get("id")!)) {
        console.warn("Workspace not being watched", workspace.get("id")!);
        return;
      }

      await unwatchWorkspace(workspace.get("id")!);
      this.workspaces.delete(workspace.get("id")!);
    }
  }

  public getWorkspaces(): WorkspaceState[] {
    return Array.from(this.workspaces.values())
      .filter((result) => result.isOk())
      .map((result) => result.unwrap());
  }

  public getWorkspaceInfo(workspaceId: string): Option<Result<WorkspaceState, WorkspaceError>> {
    return Some(this.workspaces.get(workspaceId));
  }
}
