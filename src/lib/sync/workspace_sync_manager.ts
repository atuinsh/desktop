import { useStore } from "@/state/store";
import Logger from "../logger";
import Mutex from "../std/mutex";
import { User } from "@/state/models";
import { DateTime } from "luxon";
import { autobind } from "../decorators";
import * as api from "@/api/api";
import { processUnprocessedOperations } from "@/state/runbooks/operation_processor";
import { ConnectionState } from "@/state/store/user_state";
import Workspace from "@/state/runbooks/workspace";
import { getGlobalOptions } from "../global_options";
import ServerNotificationManager from "@/server_notification_manager";

type Store = typeof useStore;

const SYNC_CHECK_INTERVAL = 10_000;
const NORMAL_SYNC_INTERVAL_SECS = 10 * 60;
const EARLY_SYNC_INTERVAL_SECS = 30;

/**
 * Manages workspace synchronization.
 */
export default class WorkspaceSyncManager {
  private static syncMutexes: Map<string, Mutex> = new Map();
  private static instance: WorkspaceSyncManager | null = null;

  public static get(store: Store): WorkspaceSyncManager {
    if (!WorkspaceSyncManager.instance) {
      WorkspaceSyncManager.instance = new WorkspaceSyncManager(store);
    }

    return WorkspaceSyncManager.instance;
  }

  private readonly logger: Logger = new Logger("SyncManager", "#ff33cc", "#ff6677");
  private store: Store;
  private notifications: ServerNotificationManager;
  private handlers: Function[] = [];
  private syncing: boolean = false;
  private lastSync: DateTime | null = null;
  private startNextSyncEarly: boolean = false;
  private connectionState: ConnectionState;
  private focused: boolean | null = null;
  private periodicSyncTimeout: Timeout | null = null;

  private constructor(store: Store) {
    this.store = store;
    this.notifications = ServerNotificationManager.get();

    this.connectionState = store.getState().connectionState;
    this.handlers.push(
      this.store.subscribe((state) => state.connectionState, this.handleConnectionStateChange),
    );
    this.handlers.push(this.store.subscribe((state) => state.user, this.handleUserChange));
    this.handlers.push(
      this.store.subscribe((state) => state.focused, this.handleFocusedChange, {
        fireImmediately: true,
      }),
    );

    this.handlers.push(
      this.notifications.onOrgEvent(() => {
        this.startSync();
      }),
    );

    this.periodicSyncCheck();
  }

  @autobind
  private async periodicSyncCheck() {
    if (this.periodicSyncTimeout) {
      clearTimeout(this.periodicSyncTimeout);
      this.periodicSyncTimeout = null;
    }

    if (this.shouldSync()) {
      this.startSync();
    }

    this.periodicSyncTimeout = setTimeout(this.periodicSyncCheck, SYNC_CHECK_INTERVAL);
  }

  public static syncMutex(runbookId: string) {
    let mutex: Mutex;
    if (WorkspaceSyncManager.syncMutexes.has(runbookId)) {
      mutex = WorkspaceSyncManager.syncMutexes.get(runbookId)!;
    } else {
      mutex = new Mutex();
      WorkspaceSyncManager.syncMutexes.set(runbookId, mutex);
    }

    mutex.once("free").then(() => {
      WorkspaceSyncManager.syncMutexes.delete(runbookId);
    });

    return mutex;
  }

  public async startSync() {
    if (this.syncing) {
      throw new Error("Sync already in progress");
    }

    if (this.connectionState !== ConnectionState.Online) return;

    this.syncing = true;
    this.startNextSyncEarly = false;

    try {
      this.store.getState().setIsSyncing(true);
      await this.store.getState().refreshUser();
      // The operation processor may create server models, so it needs to run first.
      await processUnprocessedOperations();
      // `processUnprocessedOperations` will exit early if we're offline;
      // double check before we continue.
      if (this.connectionState !== ConnectionState.Online) {
        this.logger.error("Syncing while offline; aborting");
        return;
      }

      // Before we sync the runbooks, we need to ensure that all the
      // workspaces from the server exist locally. We don't need to
      // send local workspaces to the server because we use the
      // operation system for that.
      await this.syncWorkspaces();

      this.lastSync = DateTime.now();
    } catch (err: any) {
      this.logger.error(`Synchronizer threw an error: ${err}`);
    } finally {
      this.store.getState().setIsSyncing(false);
      this.store.getState().refreshRunbooks();
      this.syncing = false;
    }
  }

  private async syncWorkspaces() {
    const serverWorkspaces = await api.getWorkspaces();
    const localWorkspaces = await Workspace.all();

    const localWorkspaceIds = new Set(localWorkspaces.map((w) => w.get("id")!));
    const serverWorkspaceIds = new Set(serverWorkspaces.map((w) => w.id));

    const idsToCreate = Array.from(serverWorkspaceIds).filter((id) => !localWorkspaceIds.has(id));
    const idsToUpdate = Array.from(serverWorkspaceIds).filter((id) => localWorkspaceIds.has(id));
    const idsToDelete = Array.from(localWorkspaceIds).filter((id) => !serverWorkspaceIds.has(id));

    for (const id of idsToCreate) {
      const workspace = serverWorkspaces.find((w) => w.id === id);
      if (workspace) {
        const ws = new Workspace({
          id: workspace.id,
          name: workspace.name,
          orgId: workspace.owner.type === "org" ? workspace.owner.id : null,
          permissions: workspace.permissions,
          online: 1,
        });
        await ws.save();
        if (workspace.owner.type === "org") {
          this.notifications.emit("org_workspace_created", {
            org_id: workspace.owner.id,
            workspace_id: workspace.id,
          });
        } else {
          this.notifications.emit("workspace_created", { id: workspace.id });
        }
      }
    }

    for (const id of idsToUpdate) {
      const localWorkspace = localWorkspaces.find((w) => w.get("id")! === id);
      const serverWorkspace = serverWorkspaces.find((w) => w.id === id);
      if (localWorkspace && serverWorkspace) {
        localWorkspace.set("name", serverWorkspace.name);
        localWorkspace.set("permissions", serverWorkspace.permissions);
        await localWorkspace.save();
        if (serverWorkspace.owner.type === "org") {
          this.notifications.emit("org_workspace_updated", {
            org_id: serverWorkspace.owner.id,
            workspace_id: serverWorkspace.id,
          });
        } else {
          this.notifications.emit("workspace_updated", { id: serverWorkspace.id });
        }
      }
    }

    for (const id of idsToDelete) {
      const workspace = localWorkspaces.find((w) => w.get("id")! === id);
      // Only propagate deletes for online workspaces;
      // offline workspaces will need to be deleted on each synced client.
      if (workspace?.isOnline() || workspace?.isLegacyHybrid()) {
        await workspace.del();
        this.notifications.emit("org_workspace_deleted", {
          org_id: workspace.get("orgId")!,
          workspace_id: workspace.get("id")!,
        });
      }
    }
  }

  private shouldSync(): boolean {
    const syncInterval = this.startNextSyncEarly
      ? EARLY_SYNC_INTERVAL_SECS
      : NORMAL_SYNC_INTERVAL_SECS;
    return (
      this.connectionState === ConnectionState.Online &&
      !!this.focused &&
      !this.syncing &&
      !getGlobalOptions().noSync &&
      this.secondsSinceLastSync() >= syncInterval
    );
  }

  private secondsSinceLastSync(): number {
    return this.lastSync ? DateTime.now().diff(this.lastSync, "seconds").seconds : Infinity;
  }

  @autobind
  private async handleUserChange(newUser: User, lastUser: User) {
    if (newUser.is(lastUser)) return;

    this.logger.info("Current user changed; resyncing all runbooks");
    if (this.syncing) {
      this.syncing = false;
    }

    this.startNextSyncEarly = true;
    this.periodicSyncCheck();
  }

  @autobind
  private handleConnectionStateChange(connectionState: ConnectionState) {
    if (connectionState === this.connectionState) return;
    this.connectionState = connectionState;

    if (connectionState === ConnectionState.Online) {
      this.logger.debug("Connection to server established");
      this.startNextSyncEarly = true;
      if (this.focused) {
        // Prevent sycning for 10 seconds to allow the connection to settle
        // and let the next periodic check pick up the sync
        this.lastSync = DateTime.now().minus({
          seconds: EARLY_SYNC_INTERVAL_SECS - SYNC_CHECK_INTERVAL / 1000 + 1,
        });

        // Reset the timer
        this.periodicSyncCheck();
      }
    } else {
      this.logger.debug("Connection to server lost");
    }
  }

  @autobind
  private handleFocusedChange(focused: boolean) {
    if (focused === this.focused) return;
    this.focused = focused;
  }
}
