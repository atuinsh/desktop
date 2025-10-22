import Workspace from "@/state/runbooks/workspace";
import WorkspaceStrategy, { DoFolderOp } from "./strategy";
import Operation, {
  createFolder,
  createRunbook,
  createWorkspace,
  deleteFolder,
  deleteRunbook,
  moveItems,
  renameWorkspace,
  updateFolderName,
} from "@/state/runbooks/operation";
import { Err, None, Ok, Result, Some } from "@binarymuse/ts-stdlib";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import { DialogBuilder } from "@/components/Dialogs/dialog";
import * as api from "@/api/api";
import { useStore } from "@/state/store";
import track_event from "@/tracking";
import doWorkspaceFolderOp from "@/state/runbooks/workspace_folder_ops";
import { WorkspaceError } from "@/rs-bindings/WorkspaceError";
import { uuidv7 } from "uuidv7";
import { NodeApi } from "react-arborist";
import { TreeRowData } from "@/components/runbooks/List/TreeView";
import { ydocToBlocknote } from "../ydoc_to_blocknote";
import * as Y from "yjs";
import Snapshot from "@/state/runbooks/snapshot";

export default class OnlineStrategy implements WorkspaceStrategy {
  constructor(private workspace: Workspace) {}

  async createWorkspace(): Promise<Result<Workspace, WorkspaceError>> {
    try {
      await this.workspace.save();

      const op = new Operation({
        operation: createWorkspace(
          this.workspace.get("id")!,
          this.workspace.get("name")!,
          this.workspace.isOrgOwned()
            ? { type: "org", orgId: this.workspace.get("orgId")! }
            : { type: "user" },
        ),
      });
      await op.save();
    } catch (err) {
      if (err instanceof Error) {
        return Err({
          type: "WorkspaceCreateError",
          data: {
            workspace_id: this.workspace.get("id")!,
            message: err.message,
          },
        } as WorkspaceError);
      } else {
        return Err({
          type: "WorkspaceCreateError",
          data: {
            workspace_id: this.workspace.get("id")!,
            message: "An unknown error occurred while creating the workspace.",
          },
        } as WorkspaceError);
      }
    }

    let type = this.workspace.isOrgOwned() ? "online-org" : "online-user";
    track_event("workspace.create", {
      type,
    });

    return Ok(this.workspace);
  }

  async renameWorkspace(newName: string): Promise<Result<undefined, WorkspaceError>> {
    try {
      this.workspace.set("name", newName);
      await this.workspace.save();

      const op = new Operation({
        operation: renameWorkspace(this.workspace.get("id")!, newName),
      });
      await op.save();
      return Ok(undefined);
    } catch (err) {
      if (err instanceof Error) {
        return Err({
          type: "WorkspaceRenameError",
          data: {
            workspace_id: this.workspace.get("id")!,
            message: err.message,
          },
        } as WorkspaceError);
      } else {
        return Err({
          type: "WorkspaceRenameError",
          data: {
            workspace_id: this.workspace.get("id")!,
            message: "An unknown error occurred while renaming the workspace.",
          },
        } as WorkspaceError);
      }
    }
  }

  async deleteWorkspace(): Promise<void> {
    this.workspace.del();
  }

  async createRunbook(
    parentFolderId: string | null,
    activateRunbook: (runbookId: string) => Promise<void>,
  ): Promise<Result<string, WorkspaceError>> {
    const rb = await OnlineRunbook.createUntitled(this.workspace, true);
    await this.onRunbookCreated(rb, parentFolderId, activateRunbook);
    return Ok(rb.id);
  }

  async importRunbookFromHub(
    runbookId: string,
    _tag: string, // since we're online, we import all tags
    activateRunbook: (runbookId: string) => Promise<void>,
  ): Promise<Result<string, WorkspaceError>> {
    const remoteRunbook = await api.getRunbookID(runbookId);

    const latestTagContent = await api.getRunbookYdoc(runbookId);
    const doc = new Y.Doc();
    if (latestTagContent) {
      Y.applyUpdate(doc, latestTagContent);
    }
    const blocknoteContent = await ydocToBlocknote(doc);

    const rb = await OnlineRunbook.createUntitled(this.workspace, true);
    if (remoteRunbook.name) {
      rb.name = remoteRunbook.name;
    }
    rb.ydoc = latestTagContent;
    rb.source = "local";
    rb.forkedFrom = runbookId;
    rb.content = JSON.stringify(blocknoteContent);
    await rb.save();

    // `onRunbookCreated` also creates the runbook on the server.
    const success = await this.onRunbookCreated(rb, null, activateRunbook);
    if (!success) {
      return Err({
        type: "RunbookSaveError",
        data: {
          runbook_id: rb.id,
          message: "Failed to create runbook",
        },
      } as WorkspaceError);
    }

    for (const snapshot of remoteRunbook.snapshots) {
      const remoteSnapshot = await api.getSnapshotById(snapshot.id);
      const localSnapshot = await Snapshot.create({
        id: uuidv7(),
        tag: remoteSnapshot.tag,
        runbook_id: rb.id,
        content: JSON.stringify(remoteSnapshot.content),
      });

      await api.createSnapshot(localSnapshot);
    }

    track_event("runbooks.import", {
      workspaceType: "online",
      forkedFrom: runbookId,
    });

    return Ok(rb.id);
  }

  async deleteRunbook(
    doFolderOp: DoFolderOp,
    runbookId: string,
  ): Promise<Result<undefined, WorkspaceError>> {
    // First, try to load and delete the runbook from the database
    const runbook = await OnlineRunbook.load(runbookId);
    if (runbook) {
      try {
        await runbook.delete();
      } catch (err) {
        console.error("Failed to delete runbook from database", err);
        // Continue anyway to remove from folder tree
      }
    }

    // Remove the runbook from the workspace folder tree
    const success = await doFolderOp(
      (wsf) => {
        return wsf.deleteRunbook(runbookId);
      },
      (changeRef) => {
        return Some(deleteRunbook(this.workspace.get("id")!, runbookId, changeRef));
      },
    );

    if (success) {
      return Ok(undefined);
    } else {
      return Err({
        type: "RunbookDeleteError",
        data: {
          workspace_id: this.workspace.get("id")!,
          runbook_id: runbookId,
          message: "Failed to delete runbook",
        },
      } as WorkspaceError);
    }
  }

  async createFolder(
    doFolderOp: DoFolderOp,
    parentId: string | null,
    name: string,
  ): Promise<Result<string, WorkspaceError>> {
    const id = uuidv7();
    const success = await doFolderOp(
      (wsf) => wsf.createFolder(id, "New Folder", parentId),
      (changeRef) => {
        return Some(createFolder(this.workspace.get("id")!, parentId, id, "New Folder", changeRef));
      },
    );

    if (success) {
      return Ok(id);
    } else {
      return Err({
        type: "FolderCreateError",
        data: {
          workspace_id: this.workspace.get("id")!,
          name: name,
          message: "Failed to create folder",
        },
      } as WorkspaceError);
    }
  }

  async renameFolder(
    doFolderOp: DoFolderOp,
    folderId: string,
    newName: string,
  ): Promise<Result<undefined, WorkspaceError>> {
    let success = await doFolderOp(
      (wsf) => wsf.renameFolder(folderId, newName),
      (changeRef) => {
        return Some(updateFolderName(this.workspace.get("id")!, folderId, newName, changeRef));
      },
    );

    if (!success) {
      return Err({
        type: "FolderRenameError",
        data: {
          workspace_id: this.workspace.get("id")!,
          folder_id: folderId,
          message: "Failed to rename folder",
        },
      } as WorkspaceError);
    }

    return Ok(undefined);
  }

  async deleteFolder(
    doFolderOp: DoFolderOp,
    folderId: string,
    descendents: NodeApi<TreeRowData>[],
  ): Promise<Result<undefined, WorkspaceError>> {
    const runbookIdsToDelete = descendents
      .filter((child) => child.data.type === "runbook")
      .map((child) => child.data.id);

    const promises = runbookIdsToDelete.map(async (runbookId) => {
      const runbook = await OnlineRunbook.load(runbookId);
      if (runbook) {
        return runbook.delete();
      } else {
        return Promise.resolve();
      }
    });

    await Promise.allSettled(promises);

    const success = await doFolderOp(
      (wsf) => wsf.deleteFolder(folderId),
      (changeRef) => {
        return Some(deleteFolder(this.workspace.get("id")!, folderId, changeRef));
      },
    );

    if (success) {
      return Ok(undefined);
    } else {
      return Err({
        type: "FolderDeleteError",
        data: {
          workspace_id: this.workspace.get("id")!,
          folder_id: folderId,
          message: "Failed to delete folder",
        },
      } as WorkspaceError);
    }
  }

  async moveItems(
    doFolderOp: DoFolderOp,
    ids: string[],
    parentId: string | null,
    index: number,
  ): Promise<Result<undefined, WorkspaceError>> {
    doFolderOp(
      (wsf) => wsf.moveItems(ids, parentId, index),
      (changeRef) => {
        return Some(moveItems(this.workspace.get("id")!, ids, parentId, index, changeRef));
      },
    );

    return Ok(undefined);
  }

  private async onRunbookCreated(
    runbook: Runbook,
    parentFolderId: string | null,
    activateRunbook: (runbookId: string) => Promise<void>,
  ): Promise<boolean> {
    // NOTE [mkt]:
    // This API call is made here instead of through the operation processor
    // because we need to wait for the runbook to be created on the server
    // before opening it; this is so the server observer doesn't create an
    // observer for the runbook and cause a YJS sync conflict.
    //
    // Note that this requires the new runbook to be opened immediately,
    // so that we create a PhoenixProvider via RunbookEditor,
    // which is why we call `activateRunbook` before updating
    // the workspace folder (which would trigger the server observer).
    //
    // Otherwise, the server observer would hold the lock to the
    // runbook provider and prevent the runbook from opening for several seconds.
    const workspace = await Workspace.get(runbook.workspaceId);
    if (!workspace) {
      return false;
    }

    let startedSyncIndicator = false;
    try {
      // TODO: use sync increment/decrement???
      if (!useStore.getState().isSyncing) {
        startedSyncIndicator = true;
        useStore.getState().setIsSyncing(true);
      }

      await api.createRunbook(runbook, runbook.id, "private");
    } catch (err) {
      if (err instanceof api.HttpResponseError) {
        new DialogBuilder()
          .title("Failed to create online runbook")
          .message("The API request to create the runbook failed.")
          .action({ label: "OK", value: "ok", variant: "flat" })
          .build();
      } else {
        new DialogBuilder()
          .title("Failed to create runbook")
          .message("You may be offline, or the server may be down.")
          .action({ label: "OK", value: "ok", variant: "flat" })
          .build();
      }
      console.error(err);
      runbook.delete();
      return false;
    } finally {
      if (startedSyncIndicator) {
        useStore.getState().setIsSyncing(false);
      }
    }

    await activateRunbook(runbook.id);

    doWorkspaceFolderOp(
      workspace.get("id")!,
      (wsf) => {
        wsf.createRunbook(runbook.id, parentFolderId);
        return true;
      },
      (changeRef) => {
        if (workspace && workspace.isOnline()) {
          return Some(createRunbook(workspace.get("id")!, parentFolderId, runbook.id, changeRef));
        } else {
          return None;
        }
      },
    );

    return true;
  }
}
