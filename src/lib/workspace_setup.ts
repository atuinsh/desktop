import Workspace from "@/state/runbooks/workspace";
import { useStore } from "@/state/store";
import * as api from "@/api/api";
import Operation from "@/state/runbooks/operation";
import { OnlineRunbook } from "@/state/runbooks/runbook";
import LegacyWorkspace from "@/state/runbooks/legacy_workspace";
import WorkspaceFolder, { Folder } from "@/state/runbooks/workspace_folders";
import { SharedStateManager } from "./shared_state/manager";
import { AtuinSharedStateAdapter } from "./shared_state/adapter";
import { uuidv7 } from "uuidv7";
import Logger from "./logger";
import { Rc } from "@binarymuse/ts-stdlib";
import { TabIcon } from "@/state/store/ui_state";
import { invoke } from "@tauri-apps/api/core";
import { DialogBuilder } from "@/components/Dialogs/dialog";

const logger = new Logger("WorkspaceMigration");

let resolve: (() => void) | null = null;
let setupPromise: Promise<void> | null = null;

export default async function doWorkspaceSetup(): Promise<void> {
  // This is a bit of a hack to ensure that we only run the workspace setup once,
  // even if React runs the effect twice.
  if (setupPromise) {
    return setupPromise;
  }

  setupPromise = new Promise<void>((res) => {
    resolve = res;
  });

  const { currentWorkspaceId, setCurrentWorkspaceId } = useStore.getState();

  // Ensure at least one workspace exists
  const workspaces = await Workspace.all();
  const legacyWorkspaces = await LegacyWorkspace.all();
  if (workspaces.length === 0) {
    logger.info("No workspaces found; attempting to fetch from server...");
    let workspace;
    try {
      const server_wss = await api.getWorkspaces();
      const server_ws = server_wss.find((ws) => ws.owner.type === "user");
      if (!server_ws) {
        throw new Error("No user workspace found");
      }

      workspace = new Workspace({
        id: server_ws.id,
        name: server_ws.name,
        permissions: server_ws.permissions,
        online: 1,
      });
      await workspace.save();
    } catch (err) {
      logger.info("Unable to fetch workspaces from server; creating default workspace");

      let workspacePath: string | null = null;
      let welcomeWorkspaceId = uuidv7();
      try {
        workspacePath = await invoke<string>("copy_welcome_workspace", { id: welcomeWorkspaceId });
      } catch (err) {
        await new DialogBuilder()
          .title("Failed to create welcome workspace")
          .icon("error")
          .message(err as string)
          .action({ label: "OK", value: "ok", variant: "flat" })
          .build();

        resolve?.();
        return;
      }

      let name = "Welcome to Atuin";
      workspace = new Workspace({
        id: welcomeWorkspaceId,
        name: name,
        folder: workspacePath,
        online: 0,
      });
      await workspace.save();
    }

    useStore
      .getState()
      .openTab(
        "/runbook/01998818-ecf4-7993-817b-cc7dcfa180c4",
        "Welcome to Atuin!",
        TabIcon.RUNBOOKS,
      );

    workspaces.push(workspace);

    // Set the workspace_id for ALL runbooks to the newly created workspace
    // TODO
    // await Runbook.updateAll({ workspaceId: workspace.get("id")! });

    // Create the default organization structure based off the legacy workspaces
    const runbooks = (await OnlineRunbook.allInAllWorkspaces()) as OnlineRunbook[];

    if (legacyWorkspaces.length > 0 || runbooks.length > 0) {
      const stateManager = SharedStateManager.getInstance<Folder>(
        `workspace-folder:${workspace.get("id")}`,
        new AtuinSharedStateAdapter(`workspace-folder:${workspace.get("id")}`),
      );

      const changeRef = await stateManager.updateOptimistic((data) => {
        const legacyWorkspaceIds = new Set<string>(legacyWorkspaces.map((ws) => ws.id));
        const folder = WorkspaceFolder.fromJS(data);

        for (const legacyWs of legacyWorkspaces) {
          folder.createFolder(legacyWs.id, legacyWs.name, null);
        }

        for (const rb of runbooks) {
          const parentId = legacyWorkspaceIds.has(rb.legacyWorkspaceId)
            ? rb.legacyWorkspaceId
            : null;
          folder.createRunbook(rb.id, parentId);
        }

        return folder.toJS();
      });

      if (changeRef) {
        // Create an operation detailing the initial workspace folder layout
        const op = new Operation({
          operation: {
            type: "workspace_initial_folder_layout",
            workspaceId: workspace.get("id")!,
            changeRef: changeRef,
            data: stateManager.data,
          },
        });
        await op.save();
      }

      Rc.dispose(stateManager);
    }
  }

  if (!currentWorkspaceId || !workspaces.some((ws) => ws.get("id") === currentWorkspaceId)) {
    setCurrentWorkspaceId(workspaces[0].get("id")!);
  }

  resolve?.();
}
