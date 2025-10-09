import Operation, { OperationData } from "@/state/runbooks/operation";
import WorkspaceFolder, { Folder } from "@/state/runbooks/workspace_folders";
import { useMemo, useCallback } from "react";
import { ChangeRef } from "../shared_state/types";
import useSharedState from "../shared_state/useSharedState";
import { doFolderOp } from "@/state/runbooks/workspace_folder_ops";
import { Option } from "@binarymuse/ts-stdlib";

type FolderOpFn = (
  op: (wsf: WorkspaceFolder, cancel: () => undefined) => boolean,
  operation: (changeRef: ChangeRef) => Option<OperationData>,
) => Promise<boolean>;

/**
 * Returns a tuple of the current {@link WorkspaceFolder} and a function for performing operations on it.
 *
 * The function takes two arguments:
 * 1. A function that accepts the current {@link WorkspaceFolder} and a cancel function as arguments,
 * and returns a boolean indicating whether the {@link WorkspaceFolder} operation was successful.
 * 2. A function that accepts a {@link ChangeRef} and returns an {@link OperationData} object.
 *
 * If the first function returns boolean `true`, an {@link Operation} is created from the
 * returned data, is saved to the database, and sent to the shared state adapter.
 *
 * ## Example
 *
 * ```ts
 * const [workspaceFolder, doFolderOp] = useWorkspaceFolder(workspaceId);
 *
 * const createFolder = (parentId: string | null, name: string) => {
 *   const id = uuidv7();
 *   doFolderOp(
 *     (wsf) => wsf.createFolder(id, name, parentId),
 *     (changeRef) => ({ type: "create_folder", data: { id, name, parentId, changeRef } })
 *   );
 * }
 * ```
 *
 * @see {@link FolderOpFn}
 */
export default function useWorkspaceFolder(workspaceId: string): [WorkspaceFolder, FolderOpFn] {
  const [folderState, updateFolderState] = useSharedState<Folder>(
    `workspace-folder:${workspaceId}`,
  );

  const workspaceFolder = useMemo(() => {
    console.log("folderState", folderState);
    return WorkspaceFolder.fromJS(folderState);
  }, [folderState]);

  const wrappedDoFolderOp = useCallback(
    async (
      op: (wsf: WorkspaceFolder, cancel: () => undefined) => boolean,
      operation: (changeRef: ChangeRef) => Option<OperationData>,
    ) => {
      const result = await doFolderOp(updateFolderState, op, operation);
      return result.success;
    },
    [updateFolderState],
  );

  return [workspaceFolder, wrappedDoFolderOp];
}
