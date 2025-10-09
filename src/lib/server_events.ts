import { AtuinStore } from "@/state/store";
import ServerNotificationManager, { OrgWorkspaceNotification } from "@/server_notification_manager";
import * as api from "@/api/api";
import Workspace from "@/state/runbooks/workspace";

export function setupServerEvents(
  store: AtuinStore,
  notificationManager: ServerNotificationManager,
) {
  const queryClient = store.getState().queryClient;

  notificationManager.on("runbook_updated", (runbookId: string) => {
    queryClient.invalidateQueries({ queryKey: ["remote_runbook", runbookId] });
  });

  notificationManager.on("runbook_deleted", (runbookId: string) => {
    queryClient.invalidateQueries({ queryKey: ["remote_runbook", runbookId] });
  });

  notificationManager.on("collab_invited", async (collabId: string) => {
    try {
      const collab = await api.getCollaborationById(collabId);
      store.getState().addCollaboration(collab);
    } catch (err) {}
  });

  notificationManager.on("collab_accepted", async (collabId: string) => {
    store.getState().markCollaborationAccepted(collabId);
    try {
      const collab = await api.getCollaborationById(collabId);
      queryClient.invalidateQueries({ queryKey: ["remote_runbook", collab.runbook.id] });
    } catch (err) {}
  });

  notificationManager.on("collab_deleted", async (collabId: string) => {
    const { collaborations, removeCollaboration } = store.getState();
    const collaboration = collaborations.find((c) => c.id === collabId);
    if (collaboration) {
      queryClient.invalidateQueries({ queryKey: ["remote_runbook", collaboration.runbook.id] });
    }
    removeCollaboration(collabId);
  });

  notificationManager.on("org_workspace_created", async (params: OrgWorkspaceNotification) => {
    try {
      const workspace = await Workspace.get(params.workspace_id);
      if (workspace) return;

      const serverWorkspace = await api.getWorkspace(params.workspace_id);
      if (!serverWorkspace || serverWorkspace.owner.type !== "org") return;

      const newWorkspace = new Workspace({
        id: serverWorkspace.id,
        name: serverWorkspace.name,
        orgId: serverWorkspace.owner.id,
        online: 1,
      });

      await newWorkspace.save();
    } catch (err) {
      console.error("Error creating org workspace", err);
    }
  });

  notificationManager.on("org_workspace_updated", async (params: OrgWorkspaceNotification) => {
    try {
      const workspace = await Workspace.get(params.workspace_id);
      if (!workspace) return;

      const serverWorkspace = await api.getWorkspace(params.workspace_id);
      if (!serverWorkspace) return;

      workspace.set("name", serverWorkspace.name);
      await workspace.save();
    } catch (err) {
      console.error("Error updating org workspace", err);
    }
  });

  notificationManager.on("org_workspace_deleted", async (params: OrgWorkspaceNotification) => {
    try {
      const workspace = await Workspace.get(params.workspace_id);
      if (!workspace) return;

      await workspace.del();
    } catch (err) {
      console.error("Error deleting org workspace", err);
    }
  });
}
