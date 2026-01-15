import SocketManager, { WrappedChannel } from "./socket";
import Runbook from "./state/runbooks/runbook";
import Emittery from "emittery";
import Workspace from "./state/runbooks/workspace";

export type RunbookNotification = {
  id: string;
};

export type WorkspaceNotification = {
  id: string;
};

export type OrgWorkspaceNotification = {
  org_id: string;
  workspace_id: string;
};

export type OrgNotification = {
  id: string;
};

/**
 * Connects to the Hub via websocket and requests notifications for changes
 * to subscribed runbooks.
 *
 * @emits `"runbook_created", runbookId`
 * @emits `"runbook_updated", runbookId`
 * @emits `"runbook_deleted", runbookId`
 *
 * @emits `"collab_invited", collabId`
 * @emits `"collab_accepted", collabId`
 * @emits `"collab_deleted", collabId`
 *
 * @emits `"org_joined", orgId`
 * @emits `"org_left", orgId`
 * @emits `"org_updated", orgId`
 * @emits `"org_deleted", orgId`
 *
 * @emits `"org_workspace_created", orgId, workspaceId`
 * @emits `"org_workspace_updated", orgId, workspaceId`
 * @emits `"org_workspace_deleted", orgId, workspaceId`
 */
export default class ServerNotificationManager extends Emittery {
  static instance: ServerNotificationManager;
  private manager: SocketManager;
  private channel: Option<WrappedChannel> = None;

  static get() {
    if (!ServerNotificationManager.instance) {
      ServerNotificationManager.instance = new ServerNotificationManager();
    }

    return ServerNotificationManager.instance;
  }

  constructor() {
    super();
    this.manager = SocketManager.get();
    this.manager.onConnect(() => {
      this.channel = Some(this.manager.channel("notifications"));
      this.startNotifications();
    });
  }

  public async startNotifications() {
    const channel = this.channel.unwrap();

    // Subscribe is idempotent on the server, so we can just call it with all the runbook IDs
    const runbookIds = await Runbook.allIdsInAllWorkspaces();
    // TODO: replace this with a proper org model one day
    const orgIds = await Workspace.allOrgIds();

    channel.push("subscribe", { ids: runbookIds });
    channel.push("subscribe_orgs", { ids: orgIds });

    channel.on("runbook_created", (params: RunbookNotification) => {
      this.emit("runbook_created", params.id);
    });
    channel.on("runbook_updated", (params: RunbookNotification) => {
      this.emit("runbook_updated", params.id);
    });
    channel.on("runbook_deleted", (params: RunbookNotification) => {
      this.emit("runbook_deleted", params.id);
    });

    channel.on("collab_invited", (params: RunbookNotification) => {
      this.emit("collab_invited", params.id);
    });
    channel.on("collab_accepted", (params: RunbookNotification) => {
      this.emit("collab_accepted", params.id);
    });
    channel.on("collab_deleted", (params: RunbookNotification) => {
      this.emit("collab_deleted", params.id);
    });

    channel.on("org_workspace_created", (params: OrgWorkspaceNotification) => {
      this.emit("org_workspace_created", params);
    });
    channel.on("org_workspace_updated", (params: OrgWorkspaceNotification) => {
      this.emit("org_workspace_updated", params);
    });
    channel.on("org_workspace_deleted", (params: OrgWorkspaceNotification) => {
      this.emit("org_workspace_deleted", params);
    });

    channel.on("org_joined", (params: OrgNotification) => {
      this.emit("org_joined", params);
    });
    channel.on("org_left", (params: OrgNotification) => {
      this.emit("org_left", params);
    });
    channel.on("org_updated", (params: OrgNotification) => {
      this.emit("org_updated", params);
    });
    channel.on("org_deleted", (params: OrgNotification) => {
      this.emit("org_deleted", params);
    });
  }

  public onRunbookEvent(
    callback: (event: "created" | "updated" | "deleted", runbookId: string) => void,
  ) {
    const unsub1 = this.on("runbook_updated", (runbookId: string) => {
      console.log("runbook_updated", runbookId);
      callback("updated", runbookId);
    });
    const unsub2 = this.on("runbook_deleted", (runbookId: string) => {
      console.log("runbook_deleted", runbookId);
      callback("deleted", runbookId);
    });
    const unsub3 = this.on("runbook_created", (runbookId: string) => {
      console.log("runbook_created", runbookId);
      callback("created", runbookId);
    });

    return () => {
      unsub1();
      unsub2();
      unsub3();
    };
  }

  public onOrgEvent(
    callback: (
      event: "joined" | "left" | "updated" | "deleted",
      eventData: { orgId: string },
    ) => void,
  ) {
    const unsub1 = this.on("org_joined", (eventData: OrgNotification) =>
      callback("joined", { orgId: eventData.id }),
    );
    const unsub2 = this.on("org_left", (eventData: OrgNotification) =>
      callback("left", { orgId: eventData.id }),
    );
    const unsub3 = this.on("org_updated", (eventData: OrgNotification) =>
      callback("updated", { orgId: eventData.id }),
    );
    const unsub4 = this.on("org_deleted", (eventData: OrgNotification) =>
      callback("deleted", { orgId: eventData.id }),
    );

    return () => {
      unsub1();
      unsub2();
      unsub3();
      unsub4();
    };
  }

  public onPersonalWorkspaceEvent(
    callback: (
      event: "created" | "updated" | "deleted",
      eventData: { workspaceId: string },
    ) => void,
  ) {
    const unsub1 = this.on("workspace_created", (eventData: WorkspaceNotification) =>
      callback("created", { workspaceId: eventData.id }),
    );
    const unsub2 = this.on("workspace_updated", (eventData: WorkspaceNotification) =>
      callback("updated", { workspaceId: eventData.id }),
    );
    const unsub3 = this.on("workspace_deleted", (eventData: WorkspaceNotification) =>
      callback("deleted", { workspaceId: eventData.id }),
    );

    return () => {
      unsub1();
      unsub2();
      unsub3();
    };
  }

  public onOrgWorkspaceEvent(
    callback: (
      event: "created" | "updated" | "deleted",
      eventData: { orgId: string; workspaceId: string },
    ) => void,
  ) {
    const makeCallback =
      (event: "created" | "updated" | "deleted") => (eventData: OrgWorkspaceNotification) =>
        callback(event, {
          orgId: eventData.org_id,
          workspaceId: eventData.workspace_id,
        });

    const unsub1 = this.on("org_workspace_created", makeCallback("created"));
    const unsub2 = this.on("org_workspace_updated", makeCallback("updated"));
    const unsub3 = this.on("org_workspace_deleted", makeCallback("deleted"));

    return () => {
      unsub1();
      unsub2();
      unsub3();
    };
  }

  /**
   * Subscribe to notifications for a specific runbook. The server ignores
   * duplicate subscriptions.
   * @param runbookId The ID of the runbook to subscribe to
   */
  public subscribe(runbookId: string) {
    this.manager.onConnect(() => {
      if (this.channel.isNone()) {
        return;
      }

      const channel = this.channel.unwrap();
      channel.push("subscribe", { ids: [runbookId] });
    });
  }

  public unsubscribe(runbookId: string) {
    this.manager.onConnect(() => {
      if (this.channel.isNone()) {
        return;
      }

      const channel = this.channel.unwrap();
      channel.push("unsubscribe", { ids: [runbookId] });
    });
  }
}
