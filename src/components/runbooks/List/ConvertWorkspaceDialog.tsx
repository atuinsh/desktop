import { AtuinSharedStateAdapter } from "@/lib/shared_state/adapter";
import { SharedStateManager } from "@/lib/shared_state/manager";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import Workspace from "@/state/runbooks/workspace";
import WorkspaceFolder, { Folder } from "@/state/runbooks/workspace_folders";
import {
  addToast,
  Button,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  Spinner,
  Tooltip,
} from "@heroui/react";
import { useCallback, useEffect, useMemo, useReducer, useState } from "react";
import * as api from "@/api/api";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import { RemoteRunbook } from "@/state/models";
import { readDir } from "@tauri-apps/plugin-fs";
import Operation, { createWorkspace, deleteRunbook } from "@/state/runbooks/operation";
import { FolderIcon, InfoIcon } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import * as commands from "@/lib/workspaces/commands";
import { findParentWorkspace } from "@/lib/workspaces/offline_strategy";
import { uuidv7 } from "uuidv7";
import WorkspaceManager from "@/lib/workspaces/manager";
import { doFolderOp } from "@/state/runbooks/workspace_folder_ops";
interface ConvertWorkspaceDialogProps {
  workspace: Workspace;
  onClose: () => void;
}

type WorkspaceInfo = {
  online: string[];
  offline: string[];
};

enum WorkspaceType {
  Online,
  Offline,
  Hybrid,
  Empty,
}

async function createServerWorkspace(workspace: Workspace) {
  try {
    await api.createUserWorkspace(workspace.get("id")!, workspace.get("name")!);
  } catch (err) {
    // If we got a 4xx error, we're online but the workspace already exists
    if (err instanceof api.HttpResponseError && err.code >= 400 && err.code < 500) {
      return;
    }

    // If creation failed because we're not online, schedule an operation to create the workspace
    // If the workspace already exists, the operation will be skipped
    const op = new Operation({
      operation: createWorkspace(workspace.get("id")!, workspace.get("name")!, {
        type: "user",
      }),
    });
    await op.save();
  }
}

async function migrateWorkspace(
  workspace: Workspace,
  workspaceInfo: WorkspaceInfo,
  selectedPath: string | null,
) {
  const workspaceType = getWorkspaceType(workspaceInfo);

  if (workspaceType === WorkspaceType.Online || workspaceType === WorkspaceType.Empty) {
    // If the workspace is detected to contain all online runbooks or is empty,
    // we can just update the workspace to be online.
    //
    // In testing, I found that some workspaces did not exist on the server.
    // Since this is an online workspace, we create it in that case.
    let remoteWorkspace: api.ServerWorkspace | null = null;
    try {
      remoteWorkspace = await api.getWorkspace(workspace.get("id")!);
    } catch (err) {}

    if (!remoteWorkspace) {
      try {
        await createServerWorkspace(workspace);
      } catch (err) {
        console.error("Failed to create server workspace during conversion", err);
      }
    }

    workspace.set("online", 1);
    await workspace.save();
  } else if (workspaceType === WorkspaceType.Offline) {
    // If the workspace is detected to contain all offline runbooks,
    // we can just move them to the selected path.
    if (!selectedPath) {
      throw new Error("Selected path is required for offline workspaces");
    }

    const result = await commands.createWorkspace(
      selectedPath,
      workspace.get("id")!,
      workspace.get("name")!,
    );
    if (result.isErr()) {
      throw result.unwrapErr();
    }

    workspace.set("online", 0);
    workspace.set("folder", selectedPath!);
    await workspace.save();

    const manager = WorkspaceManager.getInstance();
    await manager.watchWorkspace(workspace);

    for (const rbId of workspaceInfo.offline) {
      const oldRb = await Runbook.load(rbId);
      if (!oldRb) {
        // If the local runbook is not found, it's likely because there is a non-shared
        // "legacy" offline runbook in the workspace (from another machine).
        console.error(
          "Couldn't move offline runbook to new workspace; local runbook not found",
          rbId,
        );
        continue;
      }

      const result = await commands.createRunbook(
        workspace.get("id")!,
        null,
        oldRb!.name || "Untitled",
        JSON.parse(oldRb!.content || "[]"),
      );
      if (result.isErr()) {
        console.error("Failed to create runbook during conversion", result.unwrapErr());
        continue;
      }
    }
  } else if (workspaceType === WorkspaceType.Hybrid) {
    if (!selectedPath) {
      throw new Error("Selected path is required for converting hybrid workspaces");
    }

    // Ensure the workspace exists on the server
    let remoteWorkspace: api.ServerWorkspace | null = null;
    try {
      remoteWorkspace = await api.getWorkspace(workspace.get("id")!);
    } catch (err) {}

    if (!remoteWorkspace) {
      try {
        await createServerWorkspace(workspace);
      } catch (err) {
        console.error("Failed to create server workspace during conversion", err);
      }
    }

    // First create the new offline workspace on disk
    const newWorkspaceId = uuidv7();
    const result = await commands.createWorkspace(
      selectedPath,
      newWorkspaceId,
      workspace.get("name")! + " (Offline)",
    );
    if (result.isErr()) {
      throw result.unwrapErr();
    }

    // Next, create the new workspace in the database
    const newWs = new Workspace({
      id: newWorkspaceId,
      name: workspace.get("name")! + " (Offline)",
      online: 0,
      folder: selectedPath,
    });
    await newWs.save();

    // Finally, migrate the offline runbooks
    const manager = SharedStateManager.getInstance<Folder>(
      `workspace-folder:${workspace.get("id")}`,
      new AtuinSharedStateAdapter(`workspace-folder:${workspace.get("id")}`),
    );

    for (const rbId of workspaceInfo.offline) {
      const oldRb = await Runbook.load(rbId);
      // Remove the runbook from the old workspace's shared state
      try {
        const result = await doFolderOp(
          manager.updateOptimistic,
          (wsf) => {
            return wsf.deleteRunbook(rbId);
          },
          (changeRef) => Some(deleteRunbook(workspace.get("id")!, rbId, changeRef)),
        );
        if (!result.success && result.changeRef) {
          await manager.expireOptimisticUpdates([result.changeRef]);
        }
      } catch (err) {
        console.error("Failed to delete runbook during conversion", err);
      }

      if (!oldRb) {
        continue;
      }

      // Create the runbook in the new workspace
      await commands.createRunbook(
        newWs.get("id")!,
        null,
        oldRb!.name || "Untitled",
        JSON.parse(oldRb!.content || "[]"),
      );
    }

    workspace.set("online", 1);
    await workspace.save();
  }
}

/**
 * Analyzes the workspace to determine the IDs of the online and offline runbooks.
 * @param workspace The workspace to analyze.
 * @returns The workspace info containing the IDs of the online and offline runbooks.
 */
async function analyzeWorkspace(workspace: Workspace): Promise<WorkspaceInfo> {
  const manager = SharedStateManager.getInstance<Folder>(
    `workspace-folder:${workspace.get("id")}`,
    new AtuinSharedStateAdapter(`workspace-folder:${workspace.get("id")}`),
  );
  const data = await manager.getDataOnce();
  const wsf = WorkspaceFolder.fromJS(data);
  const runbooks = wsf.getRunbooks();

  let stats = {
    online: [] as string[],
    offline: [] as string[],
  };

  for (const rbId of runbooks) {
    let remoteRb: RemoteRunbook | null = null;
    try {
      remoteRb = await api.getRunbookID(rbId);
    } catch (err) {
      const rb = (await OnlineRunbook.load(rbId)) as OnlineRunbook | null;
      if (rb) {
        remoteRb = rb.remoteInfo ? (JSON.parse(rb.remoteInfo) as RemoteRunbook) : null;
      }
    }

    if (remoteRb) {
      stats.online.push(rbId);
    } else {
      stats.offline.push(rbId);
    }
  }

  return stats;
}

/**
 * Returns the type of workspace based on the workspace info.
 * @param workspaceInfo The workspace info containing the IDs of the online and offline runbooks.
 * @returns The type of workspace.
 */
function getWorkspaceType(workspaceInfo: WorkspaceInfo): WorkspaceType {
  if (workspaceInfo.online.length === 0 && workspaceInfo.offline.length === 0) {
    return WorkspaceType.Empty;
  }
  if (workspaceInfo.online.length > 0 && workspaceInfo.offline.length === 0) {
    return WorkspaceType.Online;
  }
  if (workspaceInfo.online.length === 0 && workspaceInfo.offline.length > 0) {
    return WorkspaceType.Offline;
  }

  return WorkspaceType.Hybrid;
}

interface FolderInfo {
  path: string | null;
  exists: boolean;
  hasContents: boolean;
  isAlreadyWorkspace: boolean;
  isChildOfWorkspace: boolean;
  checksComplete: boolean;
}

type FolderInfoAction =
  | { type: "setPath"; path: string | null }
  | { type: "setExists"; exists: boolean }
  | { type: "setHasContents"; hasContents: boolean }
  | { type: "setIsAlreadyWorkspace"; isAlreadyWorkspace: boolean }
  | { type: "setIsChildOfWorkspace"; isChildOfWorkspace: boolean }
  | { type: "checksComplete" };

function folderInfoReducer(state: FolderInfo, action: FolderInfoAction): FolderInfo {
  switch (action.type) {
    case "setPath":
      return {
        ...state,
        path: action.path,
        exists: false,
        hasContents: false,
        isAlreadyWorkspace: false,
        isChildOfWorkspace: false,
        checksComplete: false,
      };
    case "setExists":
      return { ...state, exists: action.exists };
    case "setHasContents":
      return { ...state, hasContents: action.hasContents };
    case "setIsAlreadyWorkspace":
      return { ...state, isAlreadyWorkspace: action.isAlreadyWorkspace };
    case "setIsChildOfWorkspace":
      return { ...state, isChildOfWorkspace: action.isChildOfWorkspace };
    case "checksComplete":
      return { ...state, checksComplete: true };
  }
}

export default function ConvertWorkspaceDialog(props: ConvertWorkspaceDialogProps) {
  const [loading, setLoading] = useState(true);
  const [converting, setConverting] = useState(false);
  const [workspaceInfo, setWorkspaceInfo] = useState<WorkspaceInfo | null>(null);
  const [folderInfo, dispatchFolderInfo] = useReducer(folderInfoReducer, {
    path: null,
    exists: false,
    hasContents: false,
    isAlreadyWorkspace: false,
    isChildOfWorkspace: false,
    checksComplete: false,
  });

  const connectionState = useStore((state) => state.connectionState);

  const workspaceType: WorkspaceType | undefined = useMemo(() => {
    if (workspaceInfo) {
      return getWorkspaceType(workspaceInfo);
    }
  }, [workspaceInfo]);

  useEffect(() => {
    analyzeWorkspace(props.workspace).then((info) => {
      setWorkspaceInfo(info);
      setLoading(false);
    });
  }, []);

  useEffect(() => {
    if (!folderInfo.path) return;
    let active = true;

    const readDirPromise = readDir(folderInfo.path);
    const findParentWorkspacePromise = findParentWorkspace(folderInfo.path);

    const promises = [readDirPromise, findParentWorkspacePromise] as const;
    Promise.all(promises).then(() => {
      dispatchFolderInfo({ type: "checksComplete" });
    });

    readDirPromise
      .then((dir) => {
        if (!active) return;

        dir = dir.filter((f) => f.name.toLowerCase() !== ".ds_store");

        dispatchFolderInfo({ type: "setExists", exists: true });

        if (dir.length > 0 && active) {
          dispatchFolderInfo({ type: "setHasContents", hasContents: true });
        }

        if (dir.some((f) => f.isFile && f.name.toLowerCase() === "atuin.toml")) {
          dispatchFolderInfo({ type: "setIsAlreadyWorkspace", isAlreadyWorkspace: true });
        }
      })
      .catch((err) => {
        console.error("Failed to read directory", err);
      });

    findParentWorkspacePromise
      .then((parentWorkspace) => {
        if (!active) return;

        if (parentWorkspace.isSome()) {
          dispatchFolderInfo({ type: "setIsChildOfWorkspace", isChildOfWorkspace: true });
        }
      })
      .catch((err) => {
        console.error("Failed to find parent workspace", err);
      });

    return () => {
      active = false;
    };
  }, [folderInfo.path]);

  const handleConvert = useCallback(async () => {
    if (!workspaceInfo) {
      return;
    }

    const type = getWorkspaceType(workspaceInfo);
    if ([WorkspaceType.Offline, WorkspaceType.Hybrid].includes(type) && !folderInfo.path) {
      return;
    }

    if (folderInfo.isAlreadyWorkspace) {
      return;
    }

    setConverting(true);
    try {
      await migrateWorkspace(props.workspace, workspaceInfo, folderInfo.path);
    } catch (err) {
      console.error("Failed to convert workspace", err);
      addToast({
        title: "Failed to convert workspace",
        description: "There was an error while converting the workspace. Please try again.",
        color: "danger",
      });
      setConverting(false);
      return;
    }

    setConverting(false);
    props.onClose();
  }, [props.workspace, workspaceInfo, folderInfo.path, props.onClose]);

  const pathIsMissing =
    (workspaceInfo?.offline.length ?? 0) > 0 &&
    (!folderInfo.path ||
      !folderInfo.exists ||
      folderInfo.isAlreadyWorkspace ||
      folderInfo.isChildOfWorkspace ||
      !folderInfo.checksComplete);

  const disabled = converting || loading || pathIsMissing;

  function handleLearnMore(e: React.MouseEvent<HTMLAnchorElement>) {
    e.preventDefault();
    openUrl("https://forum.atuin.sh/t/file-based-runbooks-are-coming/1304");
  }

  return (
    <Modal
      size="lg"
      isOpen={true}
      // prevents escape from closing the modal when converting
      onClose={converting ? undefined : props.onClose}
      isDismissable={!converting}
      hideCloseButton={converting}
    >
      <ModalContent>
        <ModalHeader>Convert Legacy Workspace</ModalHeader>
        <ModalBody className="flex gap-4">
          <p>
            The workspace {props.workspace.get("name")} is a legacy workspace and needs to be
            converted.{" "}
            <a href="#" className="text-sm text-blue-500" onClick={handleLearnMore}>
              <Tooltip content="Learn more">
                <InfoIcon className="w-4 h-4 inline-block mr-1 mb-1" />
              </Tooltip>
            </a>
          </p>

          {connectionState !== ConnectionState.Online && (
            <p className="text-warning">
              You are offline or not logged in to Atuin Hub. The conversion will proceed with cached
              data. To ensure up-to-date information, we recommend converting when online.
            </p>
          )}

          {workspaceType === WorkspaceType.Online && (
            <p>
              All the runbooks in this workspace are <strong>online</strong>. This workspace will be
              converted to an <strong>online-only</strong> workspace.
            </p>
          )}

          {workspaceType === WorkspaceType.Offline && (
            <>
              <p>
                All the runbooks in this workspace are <strong>offline</strong>. This workspace will
                be converted to an <strong>offline-only</strong> workspace.
              </p>
              <p>Please choose a folder in which to store the workspace.</p>
              <FolderPicker
                selectedPath={folderInfo.path}
                setSelectedPath={(path) => dispatchFolderInfo({ type: "setPath", path })}
                folderHasContents={folderInfo.hasContents}
                folderIsAlreadyWorkspace={folderInfo.isAlreadyWorkspace}
                folderIsChildOfWorkspace={folderInfo.isChildOfWorkspace}
                disabled={converting}
              />
            </>
          )}

          {workspaceType === WorkspaceType.Hybrid && (
            <>
              <p>
                This workspace contains both <strong>online</strong> and <strong>offline</strong>{" "}
                runbooks. A new offline-only workspace will be created to contain the offline
                runbooks.
              </p>
              <p>Please choose a folder in which to create the new offline-only workspace.</p>
              <FolderPicker
                selectedPath={folderInfo.path}
                setSelectedPath={(path) => dispatchFolderInfo({ type: "setPath", path })}
                folderHasContents={folderInfo.hasContents}
                folderIsAlreadyWorkspace={folderInfo.isAlreadyWorkspace}
                folderIsChildOfWorkspace={folderInfo.isChildOfWorkspace}
                disabled={converting}
              />
            </>
          )}

          {workspaceType === WorkspaceType.Empty && (
            <p>
              This workspace is empty. It will be converted to an <strong>online-only</strong>{" "}
              workspace.
            </p>
          )}

          {loading && <Spinner />}
        </ModalBody>
        <ModalFooter>
          <Button onPress={props.onClose} variant="flat" isDisabled={converting}>
            Cancel
          </Button>
          <Button
            onPress={handleConvert}
            color="primary"
            isDisabled={disabled}
            isLoading={converting}
          >
            {converting ? "Converting..." : "Convert"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

interface FolderPickerProps {
  selectedPath: string | null;
  setSelectedPath: (path: string | null) => void;
  folderHasContents: boolean;
  folderIsAlreadyWorkspace: boolean;
  folderIsChildOfWorkspace: boolean;
  disabled: boolean;
}

function FolderPicker(props: FolderPickerProps) {
  return (
    <>
      <div className="flex items-center">
        <Tooltip content="Select a folder to store your workspace locally">
          <Button
            isIconOnly
            className="mr-2"
            disabled={props.disabled}
            onPress={() => {
              open({
                directory: true,
              }).then((folder) => {
                props.setSelectedPath(folder);
              });
            }}
          >
            <FolderIcon />
          </Button>
        </Tooltip>
        <span className="overflow-x-auto whitespace-nowrap flex-grow">
          {!props.selectedPath && "No folder selected"}
          {props.selectedPath && props.selectedPath}
        </span>
      </div>
      {props.selectedPath && props.folderHasContents && !props.folderIsAlreadyWorkspace && (
        <span className="text-red-500 mt-2">
          The selected folder is not empty. Any conflicting files will be overwritten. We recommend
          creating a new empty folder for your workspace.
        </span>
      )}
      {props.selectedPath && props.folderHasContents && props.folderIsAlreadyWorkspace && (
        <span className="text-red-500 mt-2">
          The selected folder already contains a workspace. Please choose a different folder.
        </span>
      )}
      {props.selectedPath && !props.folderIsAlreadyWorkspace && props.folderIsChildOfWorkspace && (
        <span className="text-red-500 mt-2">
          The selected folder is a child of an existing workspace. Please choose a different folder.
        </span>
      )}
    </>
  );
}
