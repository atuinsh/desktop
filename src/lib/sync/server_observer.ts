import ServerNotificationManager from "@/server_notification_manager";
import { UserOrg } from "@/state/models";
import { AtuinStore, useStore } from "@/state/store";
import { SharedStateManager } from "../shared_state/manager";
import { AtuinSharedStateAdapter } from "../shared_state/adapter";
import WorkspaceFolder, { Folder } from "@/state/runbooks/workspace_folders";
import AsyncQueue, { ReleaseFn } from "../std/async_queue";
import RunbookSynchronizer from "./runbook_synchronizer";
import Logger from "../logger";
import Workspace from "@/state/runbooks/workspace";
import { DateTime } from "luxon";
import { Option, None, Some } from "@binarymuse/ts-stdlib";
import Runbook from "@/state/runbooks/runbook";
import { ConnectionState } from "@/state/store/user_state";
import { autobind } from "../decorators";
import AppBus from "../app/app_bus";
import { TabUri } from "@/state/store/ui_state";

const USER_SYNC_INTERVAL = 1000 * 60 * 1;

const concurrency = useStore.getState().syncConcurrency;
const queue = new AsyncQueue(concurrency);

export default class ServerObserver {
  private readonly store: AtuinStore;
  private readonly notifications: ServerNotificationManager;
  private readonly logger: Logger = new Logger("ServerObserver", "#ff33cc", "#ff6677");
  private subscriptions: Array<() => void> = [];
  private orgObservers: Map<string, OrgObserver> = new Map();
  private personalOrgObserver: Option<PersonalOrgObserver> = None;
  private userSyncTimer: Option<Timeout> = None;
  private lastUserSync: Option<DateTime> = None;

  constructor(store: AtuinStore, notifications: ServerNotificationManager) {
    this.store = store;
    this.notifications = notifications;

    this.init();

    store.subscribe(
      (state) => state.userOrgs,
      (orgs) => {
        this.updateOrgs(orgs);
      },
      {
        fireImmediately: true,
      },
    );
  }

  public stop() {
    this.logger.info("Stopping server observer");
    for (const observer of this.orgObservers.values()) {
      observer.stop();
    }

    if (this.personalOrgObserver.isSome()) {
      this.personalOrgObserver.unwrap().stop();
    }

    for (const unsub of this.subscriptions) {
      unsub();
    }

    this.orgObservers.clear();
    this.subscriptions = [];
  }

  private init() {
    const unsub1 = this.notifications.onOrgEvent((_event, _data) => {
      this.refreshUser();
    });

    this.subscriptions.push(unsub1);

    const unsub2 = this.store.subscribe(
      (state) => state.connectionState,
      this.handleConnectionStatusChange,
    );

    this.subscriptions.push(unsub2);

    this.personalOrgObserver = Some(new PersonalOrgObserver(this.store, this.notifications));
  }

  @autobind
  private handleConnectionStatusChange(status: ConnectionState, prevStatus: ConnectionState) {
    if (status === prevStatus) return;

    if (status === ConnectionState.Online) {
      this.refreshUser();
    } else {
      if (this.userSyncTimer.isSome()) {
        clearTimeout(this.userSyncTimer.unwrap());
        this.userSyncTimer = None;
      }
    }
  }

  private refreshUser() {
    if (this.userSyncTimer.isSome()) {
      clearTimeout(this.userSyncTimer.unwrap());
      this.userSyncTimer = None;
    }

    this.store
      .getState()
      .refreshUser()
      .then(() => {
        this.lastUserSync = Some(DateTime.now());
        this.userSyncTimer = Some(
          setTimeout(() => {
            this.maybeRefreshUser();
          }, USER_SYNC_INTERVAL),
        );
      });
  }

  private maybeRefreshUser() {
    this.lastUserSync.match({
      none: () => this.refreshUser(),
      some: (lastUserSync) => {
        const now = DateTime.now();
        if (now.diff(lastUserSync).toMillis() > USER_SYNC_INTERVAL) {
          this.refreshUser();
        }
      },
    });
  }

  private updateOrgs(orgs: Array<UserOrg>) {
    for (const org of orgs) {
      if (!this.orgObservers.has(org.id)) {
        this.logger.info(`Creating org observer for org ${org.id}`);
        this.orgObservers.set(org.id, new OrgObserver(org.id, this.store, this.notifications));
      }
    }

    for (const orgId of this.orgObservers.keys()) {
      if (!orgs.some((o) => o.id === orgId)) {
        const observer = this.orgObservers.get(orgId);
        if (observer) {
          this.logger.info(`Stopping org observer for ${orgId}`);
          observer.stop();
          this.orgObservers.delete(orgId);
        }
      }
    }
  }
}

class PersonalOrgObserver {
  private readonly store: AtuinStore;
  private readonly notifications: ServerNotificationManager;
  private workspaceObservers: Map<string, OrgWorkspaceObserver> = new Map();
  private subscriptions: Array<() => void> = [];
  private logger: Logger = new Logger("PersonalOrgObserver", "#ff33cc", "#ff6677");

  constructor(store: AtuinStore, notifications: ServerNotificationManager) {
    this.store = store;
    this.notifications = notifications;

    this.init();
  }

  private async init() {
    const personalOnlineWorkspaces = await Workspace.all({ orgId: null, online: 1 });
    for (const workspace of personalOnlineWorkspaces) {
      this.workspaceObservers.set(
        workspace.get("id")!,
        new OrgWorkspaceObserver(workspace.get("id")!, this.store, this.notifications),
      );
    }

    const unsub = this.notifications.onPersonalWorkspaceEvent((event, data) => {
      if (event === "created") {
        this.logger.info(`Creating workspace observer for workspace ${data.workspaceId}`);
        this.workspaceObservers.set(
          data.workspaceId,
          new OrgWorkspaceObserver(data.workspaceId, this.store, this.notifications),
        );
      } else if (event === "updated") {
        // Nothing to do ????
      } else if (event === "deleted") {
        const observer = this.workspaceObservers.get(data.workspaceId);
        if (observer) {
          this.logger.info(`Stopping workspace observer for workspace ${data.workspaceId}`);
          observer.stop();
          this.workspaceObservers.delete(data.workspaceId);
        }
      }
    });

    this.subscriptions.push(unsub);
  }

  public stop() {
    this.logger.info(`Stopping personal org observer`);
    for (const observer of this.workspaceObservers.values()) {
      observer.stop();
    }

    for (const unsub of this.subscriptions) {
      unsub();
    }

    this.workspaceObservers.clear();
  }
}

class OrgObserver {
  private readonly orgId: string;
  private readonly store: AtuinStore;
  private readonly notifications: ServerNotificationManager;
  private workspaceObservers: Map<string, OrgWorkspaceObserver> = new Map();
  private subscriptions: Array<() => void> = [];
  private logger: Logger = new Logger("OrgObserver", "#ff33cc", "#ff6677");

  constructor(orgId: string, store: AtuinStore, notifications: ServerNotificationManager) {
    this.orgId = orgId;
    this.store = store;
    this.notifications = notifications;
    this.logger.enable();

    this.init();
  }

  public stop() {
    this.logger.info(`Stopping org observer for org ${this.orgId}`);
    for (const observer of this.workspaceObservers.values()) {
      observer.stop();
    }

    for (const unsub of this.subscriptions) {
      unsub();
    }

    this.workspaceObservers.clear();
    this.subscriptions = [];
  }

  private async init() {
    const workspaces = await Workspace.all({ orgId: this.orgId, online: 1 });
    for (const workspace of workspaces) {
      this.workspaceObservers.set(
        workspace.get("id")!,
        new OrgWorkspaceObserver(workspace.get("id")!, this.store, this.notifications),
      );
    }

    const unsub = this.notifications.onOrgWorkspaceEvent((event, data) => {
      if (data.orgId !== this.orgId) return;

      switch (event) {
        case "created":
          if (this.workspaceObservers.has(data.workspaceId)) return;

          this.logger.info(
            `Creating workspace observer for workspace ${data.workspaceId} in org ${this.orgId}`,
          );
          this.workspaceObservers.set(
            data.workspaceId,
            new OrgWorkspaceObserver(data.workspaceId, this.store, this.notifications),
          );
          break;
        case "updated":
          // Nothing to do
          break;
        case "deleted":
          const observer = this.workspaceObservers.get(data.workspaceId);
          if (observer) {
            this.logger.info(
              `Stopping workspace observer for workspace ${data.workspaceId} in org ${this.orgId}`,
            );
            observer.stop();
            this.workspaceObservers.delete(data.workspaceId);
          }
          break;
      }
    });

    this.subscriptions.push(unsub);
  }
}

class OrgWorkspaceObserver {
  private readonly workspaceId: string;
  private readonly store: AtuinStore;
  private readonly notifications: ServerNotificationManager;
  private subscriptions: Array<() => void> = [];
  private runbookObservers: Map<string, OrgRunbookObserver> = new Map();
  private logger: Logger = new Logger("OrgWorkspaceObserver", "#ff33cc", "#ff6677");

  constructor(workspaceId: string, store: AtuinStore, notifications: ServerNotificationManager) {
    this.workspaceId = workspaceId;
    this.store = store;
    this.notifications = notifications;
    this.logger.enable();

    this.init();
  }

  public stop() {
    this.logger.info(`Stopping workspace observer for workspace ${this.workspaceId}`);
    for (const observer of this.runbookObservers.values()) {
      observer.stop();
    }

    for (const unsub of this.subscriptions) {
      unsub();
    }

    this.runbookObservers.clear();
    this.subscriptions = [];
  }

  private init() {
    const stateId = `workspace-folder:${this.workspaceId}`;
    const sharedState = SharedStateManager.getInstance<Folder>(
      stateId,
      new AtuinSharedStateAdapter(stateId),
    );

    let isWorking = false;
    const unsub = sharedState.subscribe(async (data: Folder) => {
      if (isWorking) return;
      isWorking = true;

      try {
        const existingRunbookIds = new Set(await Runbook.allIdsInAllWorkspaces());

        const wsf = WorkspaceFolder.fromJS(data);
        const runbookIds = new Set(wsf.getRunbooks());

        for (const runbookId of runbookIds) {
          if (this.runbookObservers.has(runbookId)) continue;

          this.logger.info(
            `Creating runbook observer for runbook ${runbookId} in workspace ${this.workspaceId}`,
          );
          const initialPriority = existingRunbookIds.has(runbookId) ? 0 : 1;
          this.runbookObservers.set(
            runbookId,
            new OrgRunbookObserver(
              this.workspaceId,
              runbookId,
              initialPriority,
              this.store,
              this.notifications,
            ),
          );
        }

        for (const runbookId of this.runbookObservers.keys()) {
          if (!runbookIds.has(runbookId)) {
            this.logger.info(
              `Stopping runbook observer for runbook ${runbookId} in workspace ${this.workspaceId}`,
            );
            this.runbookObservers.get(runbookId)?.stop();
            this.runbookObservers.delete(runbookId);
            this.deleteRunbook(runbookId);
          }
        }
      } finally {
        isWorking = false;
      }
    });

    this.subscriptions.push(unsub);
  }

  private async deleteRunbook(runbookId: string) {
    const runbook = await Runbook.load(runbookId);
    if (runbook && runbook.workspaceId !== this.workspaceId) {
      return;
    }

    if (runbook) {
      await runbook.delete();
    }

    this.store.getState().closeTabs((tab) => {
      const uri = new TabUri(tab.url);
      return uri.isRunbook() && uri.getRunbookId() === runbookId;
    });
  }
}

class OrgRunbookObserver {
  private readonly workspaceId: string;
  private readonly runbookId: string;
  private initialPriority: number;
  private readonly store: AtuinStore;
  private subscriptions: Array<() => void> = [];
  private readonly notifications: ServerNotificationManager;
  private syncing: boolean = false;
  private syncAfterSync: boolean = false;
  private isShutdown: boolean = false;
  private release: Option<ReleaseFn> = None;
  private logger: Logger = new Logger("OrgRunbookObserver", "#ff33cc", "#ff6677");

  constructor(
    workspaceId: string,
    runbookId: string,
    initialPriority: number,
    store: AtuinStore,
    notifications: ServerNotificationManager,
  ) {
    this.workspaceId = workspaceId;
    this.runbookId = runbookId;
    this.initialPriority = initialPriority;
    this.store = store;
    this.notifications = notifications;
    this.logger.enable();
    this.init();
  }

  public stop() {
    this.logger.info(
      `Stopping runbook observer for ${this.runbookId} in workspace ${this.workspaceId}`,
    );
    this.notifications.unsubscribe(this.runbookId);

    for (const unsub of this.subscriptions) {
      unsub();
    }

    this.isShutdown = true;

    this.subscriptions = [];
  }

  private init() {
    this.logger.info(`Subscribing to server events for runbook ${this.runbookId}`);
    const unsub = this.notifications.onRunbookEvent((event, id) => {
      if (id !== this.runbookId) return;

      switch (event) {
        case "created":
          this.logger.info(
            `Runbook ${this.runbookId} in workspace ${this.workspaceId} was created, syncing`,
          );
          this.syncRunbook();
          break;
        case "updated":
          this.logger.info(
            `Runbook ${this.runbookId} in workspace ${this.workspaceId} was updated, syncing`,
          );
          this.syncRunbook();
          break;
        case "deleted":
          this.logger.info(
            `Runbook ${this.runbookId} in workspace ${this.workspaceId} was deleted, updating local state`,
          );
          // TODO: delete runbook
          break;
      }
    });

    // Pretty sure this will work even if the runbook doesn't exist on the server yet
    // but should probably check
    this.notifications.subscribe(this.runbookId);

    this.subscriptions.push(unsub);

    this.syncRunbook(this.initialPriority);
  }

  private async syncRunbook(priority: number = 0) {
    if (!useStore.getState().backgroundSync) {
      this.logger.debug("Background sync is disabled, skipping");
      return;
    }

    if (this.syncing) {
      if (!this.isShutdown) {
        this.syncAfterSync = true;
      }
      this.logger.debug("Runbook sync already in progress, skipping");
      return;
    }
    this.syncing = true;

    try {
      this.release = Some(await queue.checkout(priority));
    } catch (e) {
      this.logger.error("FAILED to acquire queue lock");
      this.release = None;
      this.syncing = false;
      return;
    }

    if (this.isShutdown) {
      this.release.map((r) => r());
      this.release = None;
      this.syncing = false;
      return;
    }

    try {
      const user = this.store.getState().user;
      const rbs = new RunbookSynchronizer(this.runbookId, this.workspaceId, user);

      const tabsMatch = this.store.getState().tabs.some((tab) => {
        const uri = new TabUri(tab.url);
        return uri.isRunbook() && uri.getRunbookId() === this.runbookId;
      });

      const result = await rbs.sync(!tabsMatch);
      if (result.action === "created") {
        // Hack to reset the editor, including the Phoenix Provider
        AppBus.get().emitResetEditor(this.runbookId);
      }
    } catch (e) {
      if (this.store.getState().connectionState !== ConnectionState.Online) {
        this.logger.warn("Error syncing runbook, but we're offline, skipping");
      } else {
        this.logger.error("Error syncing runbook", e);
      }
    } finally {
      this.syncing = false;
      this.release.map((r) => r());
      this.release = None;

      if (this.syncAfterSync) {
        this.syncAfterSync = false;
        this.syncRunbook(2);
      }
    }
  }
}
