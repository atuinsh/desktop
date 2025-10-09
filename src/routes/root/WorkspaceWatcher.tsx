import WorkspaceManager from "@/lib/workspaces/manager";
import Workspace from "@/state/runbooks/workspace";
import { useEffect, useRef } from "react";

interface WorkspaceWatcherProps {
  workspace: Workspace;
}

export default function WorkspaceWatcher({ workspace }: WorkspaceWatcherProps) {
  // TODO: check if any of the following changed:
  // - online
  // - folder
  // and update with the appropriate backend
  // ....
  // maybe this should be handled by the strategy?? e.g. renameWorkspace, updatePath, etc.

  const watchCount = useRef(0);

  useEffect(() => {
    watchCount.current++;
    const currentWatchCount = watchCount.current;

    const workspaceManager = WorkspaceManager.getInstance();
    workspaceManager.watchWorkspace(workspace);

    return () => {
      // Don't unwatch if we've re-watched in the meantime
      if (watchCount.current === currentWatchCount) {
        workspaceManager.unwatchWorkspace(workspace);
      }
    };
  }, [workspace.get("id"), workspace.get("folder"), workspace.get("online")]);

  return null;
}
