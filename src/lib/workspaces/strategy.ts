import OnlineStrategy from "./online_strategy";
import OfflineStrategy from "./offline_strategy";
import Workspace from "@/state/runbooks/workspace";
import { Result } from "@binarymuse/ts-stdlib";
import WorkspaceFolder from "@/state/runbooks/workspace_folders";
import { ChangeRef } from "../shared_state/types";
import { OperationData } from "@/state/runbooks/operation";
import { WorkspaceError } from "@/rs-bindings/WorkspaceError";
import { NodeApi } from "react-arborist";
import { TreeRowData } from "@/components/runbooks/List/TreeView";

// TODO[mkt]: this could be refactored to not rely on passing `doFolderOp`
// from `useWorkspaceFolder` to the strategy methods.
export type DoFolderOp = (
  op: (wsf: WorkspaceFolder, cancel: () => undefined) => boolean,
  operation: (changeRef: ChangeRef) => Option<OperationData>,
) => Promise<boolean>;

/**
 * `WorkspaceStrategy` defines the interface for the different strategies
 * that can be used to interact with workspaces. `OnlineStrategy` is used
 * when the workspace is online, and `OfflineStrategy` is used when the
 * workspace is offline and backed by the filesystem.
 *
 * Because online workspaces are reliant on the shared state system to manage
 * folders and runbooks, many methods in this interface take a `DoFolderOp`
 * parameter. This parameter is a function that allows the strategy to perform
 * folder operations on the workspace, and should be obtained in a React component
 * via `useWorkspaceFolder`. The offline strategy simply ignores this parameter.
 */
export default interface WorkspaceStrategy {
  createWorkspace(): Promise<Result<Workspace, WorkspaceError>>;
  importRunbookFromHub(
    runbookId: string,
    tag: string,
    activateRunbook: (runbookId: string) => Promise<void>,
  ): Promise<Result<string, WorkspaceError>>;
  createRunbook(
    parentFolderId: string | null,
    activateRunbook: (runbookId: string) => Promise<void>,
  ): Promise<Result<string, WorkspaceError>>;
  deleteRunbook(
    doFolderOp: DoFolderOp,
    runbookId: string,
  ): Promise<Result<undefined, WorkspaceError>>;
  renameWorkspace(newName: string): Promise<Result<undefined, WorkspaceError>>;
  deleteWorkspace(): Promise<void>;
  createFolder(
    doFolderOp: DoFolderOp,
    parentId: string | null,
    name: string,
  ): Promise<Result<string, WorkspaceError>>;
  renameFolder(
    doFolderOp: DoFolderOp,
    folderId: string,
    newName: string,
  ): Promise<Result<undefined, WorkspaceError>>;
  deleteFolder(
    doFolderOp: DoFolderOp,
    folderId: string,
    // TODO[mkt]: depending on NodeApi is a slightly leaky abstraction
    descendents: NodeApi<TreeRowData>[],
  ): Promise<Result<undefined, WorkspaceError>>;
  moveItems(
    doFolderOp: DoFolderOp,
    ids: string[],
    parentId: string | null,
    index: number,
  ): Promise<Result<undefined, WorkspaceError>>;
}

export function getWorkspaceStrategy(workspace: Workspace): WorkspaceStrategy {
  if (workspace.isOnline()) {
    return new OnlineStrategy(workspace);
  } else {
    return new OfflineStrategy(workspace);
  }
}
