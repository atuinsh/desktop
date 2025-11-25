import Logger from "@/lib/logger";
import Workspace from "./workspace";
import { uuidv7 } from "uuidv7";
import untitledRunbook from "../runbooks/untitled.json";
import AtuinDB from "../atuin_db";
import { readTextFile } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { dbHook } from "@/lib/db_hooks";
import Snapshot from "./snapshot";
import * as commands from "@/lib/workspaces/commands";
import WorkspaceManager from "@/lib/workspaces/manager";
import { timeoutPromise } from "@/lib/utils";

const logger = new Logger("Runbook", "green", "green");

// Definition of an atrb file
// This is JSON encoded for ease of access, and may change in the future
export interface RunbookFile {
  version: number;

  id: string;
  name: string;
  created: Date;
  slug?: string;

  content: string;
}

export type RunbookSource = "local" | "hub" | "hub-dev" | "file";
export type RunbookVisibility = "private" | "public" | "unlisted";

export interface BaseRunbookAttrs {
  id: string;
  name: string;
  content: string;
  workspaceId: string;
  created?: Date;
  updated?: Date;
}

export default abstract class Runbook {
  id: string;
  workspaceId: string;
  protected _name: string;
  protected _content: string;

  protected persisted: boolean = false;
  abstract viewed_at: Date | null;

  created: Date;
  updated: Date;

  abstract source: RunbookSource;

  set name(value: string) {
    if (value !== this._name) {
      this.updated = new Date();
      this._name = value;
    }
  }

  set content(value: string) {
    if (value !== this._content) {
      this.updated = new Date();
      this._content = value;
    }
  }

  get name() {
    return this._name;
  }

  get content() {
    return this._content;
  }

  constructor(attrs: BaseRunbookAttrs, persisted: boolean = false) {
    this.id = attrs.id;
    this._name = attrs.name;
    this._content = attrs.content;
    this.created = attrs.created || new Date();
    this.updated = attrs.updated || new Date();
    this.workspaceId = attrs.workspaceId;
    this.persisted = persisted;
  }

  public static async allInAllWorkspaces(): Promise<Runbook[]> {
    const online = await OnlineRunbook.allInAllWorkspaces();
    const offline = await OfflineRunbook.allInAllWorkspaces();
    return [...online, ...offline];
  }

  public static async allIdsInAllWorkspaces(): Promise<string[]> {
    const online = await OnlineRunbook.allIdsInAllWorkspaces();
    const offline = await OfflineRunbook.allIdsInAllWorkspaces();
    return [...online, ...offline];
  }

  public static async count(): Promise<number> {
    const online = await OnlineRunbook.count();
    const offline = await OfflineRunbook.count();
    return online + offline;
  }

  public static async load(id: string, throwIfMissing: boolean = false): Promise<Runbook | null> {
    const online = await OnlineRunbook.load(id);
    if (online) {
      return online;
    }

    const offline = await OfflineRunbook.load(id);
    if (offline) {
      return offline;
    }

    if (throwIfMissing) {
      throw new Error(`Runbook ${id} not found`);
    }

    return null;
  }

  public static async allFromWorkspace(workspaceId: string): Promise<Runbook[]> {
    const workspace = await Workspace.get(workspaceId);
    if (!workspace) {
      return [];
    }

    if (workspace.isOnline()) {
      return OnlineRunbook.allFromWorkspace(workspaceId);
    } else {
      return OfflineRunbook.allFromWorkspace(workspaceId);
    }
  }

  public static async allFromOrg(orgId: string | null): Promise<Runbook[]> {
    const online = await OnlineRunbook.allFromOrg(orgId);
    const offline = await OfflineRunbook.allFromOrg(orgId);
    return [...online, ...offline];
  }

  public abstract isOnline(): boolean;
  public abstract save(): Promise<string | undefined>;
  public abstract clearRemoteInfo(): Promise<void>;
  public abstract moveTo(targetWorkspace: Workspace): Promise<void>;
  public abstract updateRemoteInfo(remoteInfo: string | null): Promise<void>;
  public abstract markViewed(): Promise<void>;
  public abstract clone(): Runbook;
  public abstract delete(): Promise<void>;
}

//// ONLINE

export interface OnlineRunbookAttrs extends BaseRunbookAttrs {
  viewed_at: Date | null;
  _ydoc: Uint8Array | null;
  source: RunbookSource;
  sourceInfo: string | null;
  forkedFrom: string | null;
  remoteInfo: string | null;
  legacyWorkspaceId: string | null;

  created: Date;
  updated: Date;
}

export class OnlineRunbook extends Runbook {
  viewed_at: Date | null;
  _ydoc: Uint8Array | null;
  _ydocChanged: boolean = false;
  source: RunbookSource;
  sourceInfo: string | null;
  forkedFrom: string | null;
  remoteInfo: string | null;
  legacyWorkspaceId: string;

  get ydoc() {
    return this._ydoc;
  }

  set ydoc(value: Uint8Array | null) {
    this._ydocChanged = true;
    this._ydoc = value;
  }

  constructor(attrs: OnlineRunbookAttrs, persisted: boolean = false) {
    super(attrs, persisted);
    this.viewed_at = attrs.viewed_at || null;
    this._ydoc = attrs._ydoc || null;
    this.source = attrs.source || "local";
    this.sourceInfo = attrs.sourceInfo || null;
    this.forkedFrom = attrs.forkedFrom || null;
    this.remoteInfo = attrs.remoteInfo || null;
    this.legacyWorkspaceId = attrs.legacyWorkspaceId || "";
  }

  public isOnline(): boolean {
    return true;
  }

  /// Create a new Runbook, and automatically generate an ID.
  public static async create(
    workspace: Workspace,
    persist: boolean = true,
  ): Promise<OnlineRunbook> {
    let now = new Date();

    // Initialize with the same value for created/updated, to avoid needing null.
    let runbook = new OnlineRunbook({
      id: uuidv7(),
      name: "",
      source: "local",
      sourceInfo: null,
      content: "",
      _ydoc: null,
      created: now,
      updated: now,
      workspaceId: workspace.get("id")!,
      legacyWorkspaceId: "",
      forkedFrom: null,
      remoteInfo: null,
      viewed_at: null,
    });

    if (persist) {
      await runbook.save();
    }

    return runbook;
  }

  public static async createUntitled(workspace: Workspace, markViewed: boolean = false) {
    if (!workspace.isOnline()) {
      throw new Error(
        "Creation of runbooks in an offline workspace needs to be done via the WorkspaceStrategy adapter",
      );
    }

    let runbook = await OnlineRunbook.create(workspace);
    runbook.name = "Untitled";
    runbook.content = JSON.stringify(untitledRunbook);
    if (markViewed) {
      runbook.viewed_at = new Date();
    }
    await runbook.save();

    return runbook;
  }

  public static async count(): Promise<number> {
    const db = await AtuinDB.load("runbooks");
    let res = await db.select<any[]>("select count(1) as count from runbooks");

    return res[0]["count"];
  }

  public async export(filePath: string) {
    // Load the runbook from the ID. This ensures we have all fields populated properly, and as up-to-date as possible.
    // TODO: we are probably going to be changing stuff here for snapshots
    let rb = await OnlineRunbook.load(this.id);
    if (!rb) return;

    let runbook = {
      id: this.id,
      name: this.name,
      created: this.created.getTime() * 1000000,
      content: rb.content,
    };

    await invoke<string>("export_atrb", {
      json: JSON.stringify(runbook),
      filePath: filePath,
    });
  }

  public static async importJSON(
    obj: RunbookFile,
    source: RunbookSource,
    sourceInfo: string | null,
    remoteInfo: string | null,
    workspace: Workspace,
  ): Promise<Runbook> {
    if (!workspace || !workspace.get("id") || !workspace.canManageRunbooks()) {
      throw new Error("Must pass a workspace with manage_runbooks permissions");
    }

    let content = typeof obj.content === "object" ? obj.content : JSON.parse(obj.content);

    let runbook = new OnlineRunbook({
      id: obj.id,
      name: obj.name,
      source: source,
      sourceInfo: sourceInfo,
      content: JSON.stringify(content),
      _ydoc: null,
      created: new Date(obj.created),
      updated: new Date(),
      workspaceId: workspace.get("id")!,
      legacyWorkspaceId: "",
      forkedFrom: null,
      remoteInfo: remoteInfo,
      viewed_at: null,
    });

    await runbook.save();

    return runbook;
  }

  public static async importFile(filePath: string, workspace: Workspace) {
    // For some reason, we're getting an ArrayBuffer here? Supposedly it should be passing a string.
    // But it's not.
    let file = await readTextFile(filePath);
    let parsed = JSON.parse(file);

    return OnlineRunbook.importJSON(parsed, "file", null, null, workspace);
  }

  public static async load(id: String): Promise<Runbook | null> {
    const db = await AtuinDB.load("runbooks");

    let res = await db.select<any[]>(
      "select id, name, source, source_info, content, created, updated, workspace_id, " +
        "legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks where id = $1",
      [id],
    );

    if (res.length == 0) return null;

    let rbRow = res[0];

    const doc: ArrayBuffer | null = await OnlineRunbook.loadYDocForRunbook(rbRow.id);
    rbRow.ydoc = doc;

    return OnlineRunbook.fromRow(rbRow);
  }

  public static async updateAll(attrs: Partial<BaseRunbookAttrs>) {
    const db = await AtuinDB.load("runbooks");
    const keys = Object.keys(attrs).map(camelCaseToSnakeCase);
    const values = Object.values(attrs);

    if (keys.length === 0) {
      return; // No attributes to update
    }

    const setClause = keys.map((key, index) => `${key} = $${index + 1}`).join(", ");
    const query = `UPDATE runbooks SET ${setClause} WHERE 1`;

    await db.execute(query, values);
  }

  public static async selectWhere(whereClause: string, bindValues?: any[]): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    const query = `SELECT * FROM runbooks WHERE ${whereClause}`;
    const rows = await db.select<any[]>(query, bindValues);

    return rows.map(OnlineRunbook.fromRow);
  }

  static fromRow(row: any): Runbook {
    let update: Uint8Array | null = null;
    if (row.ydoc) {
      // For a short period of time, the `Y.Doc` might have been stored as a string.
      if (typeof row.ydoc == "string") {
        update = Uint8Array.from(JSON.parse(row.ydoc));
      } else if (row.ydoc.byteLength > 0) {
        update = new Uint8Array(row.ydoc);
      }
    }

    return new OnlineRunbook(
      {
        id: row.id,
        name: row.name,
        source: row.source || "local",
        sourceInfo: row.source_info,
        content: row.content || "[]",
        _ydoc: update,
        created: new Date(row.created / 1000000),
        updated: new Date(row.updated / 1000000),
        workspaceId: row.workspace_id,
        legacyWorkspaceId: row.legacy_workspace_id,
        forkedFrom: row.forked_from,
        remoteInfo: row.remote_info,
        viewed_at: row.viewed_at ? new Date(row.viewed_at / 1000000) : null,
      },
      true,
    );
  }

  static async allInAllWorkspaces(): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    let res = await db.select<any[]>(
      "select id, name, source, source_info, created, updated, workspace_id, legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks " +
        "order by updated desc",
    );

    return res.map(OnlineRunbook.fromRow);
  }

  static async allIdsInAllWorkspaces(): Promise<string[]> {
    const db = await AtuinDB.load("runbooks");
    const rows = await db.select<{ id: string }[]>("select id from runbooks order by id desc");
    return rows.map((row) => row.id);
  }

  // Default to scoping by workspace
  // Reduces the chance of accidents
  // If we ever need to fetch all runbooks for all workspaces, name it allInAllWorkspace or something
  static async all(workspaceId: string): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    let res = await db.select<any[]>(
      "select id, name, source, source_info, created, updated, workspace_id, legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks " +
        "where legacy_workspace_id = $1 or legacy_workspace_id is null order by updated desc",
      [workspaceId],
    );

    return res.map(OnlineRunbook.fromRow);
  }

  static async allFromWorkspace(workspaceId: string): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    let res = await db.select<any[]>(
      "select id, name, source, source_info, created, updated, workspace_id, legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks " +
        "where workspace_id = $1 order by updated desc",
      [workspaceId],
    );

    return res.map(OnlineRunbook.fromRow);
  }

  static async allFromOrg(orgId: string | null): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    let query = `
      select r.id, r.name, r.source, r.source_info, r.created, r.updated, r.workspace_id, r.legacy_workspace_id, r.forked_from, r.remote_info, r.viewed_at 
      from runbooks r 
      join workspaces w on r.workspace_id = w.id 
    `;

    if (orgId) {
      query += ` where w.org_id = $1 `;
    }

    query += ` order by r.updated desc`;

    let res = await db.select<any[]>(query, [orgId]);

    return res.map(OnlineRunbook.fromRow);
  }

  static async withNullLegacyWorkspaces(): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");

    let runbooks = await logger.time(`Selecting all runbooks with a null workspace`, async () => {
      let res = await db.select<Runbook[]>(
        "select id, name, source, source_info, created, updated, workspace_id, legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks " +
          "where legacy_workspace_id is null or legacy_workspace_id = '' order by updated desc",
        [],
      );

      return res.map(OnlineRunbook.fromRow);
    });

    return runbooks;
  }

  static async allForkedFrom(runbookId: string): Promise<OnlineRunbook[]> {
    const db = await AtuinDB.load("runbooks");

    const res = await db.select<any[]>(
      "select id, name, source, source_info, created, updated, workspace_id, legacy_workspace_id, forked_from, remote_info, viewed_at from runbooks " +
        "where forked_from = $1 order by updated desc",
      [runbookId],
    );

    return res.map((row) => OnlineRunbook.fromRow(row) as OnlineRunbook);
  }

  public async save(): Promise<string | undefined> {
    const db = await AtuinDB.load("runbooks");
    logger.info("Saving runbook", this.id, this.name, this._ydoc);

    await db.execute(
      `insert into runbooks(id, name, content, created, updated, workspace_id, legacy_workspace_id, source, source_info, forked_from, remote_info, viewed_at)
          values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)

          on conflict(id) do update
            set
              name=$2,
              content=$3,
              updated=$5,
              workspace_id=$6,
              legacy_workspace_id=$7,
              source=$8,
              source_info=$9,
              forked_from=$10,
              remote_info=$11,
              viewed_at=$12`,

      // getTime returns a timestamp as unix milliseconds
      // we won't need or use the resolution here, but elsewhere Atuin stores timestamps in sqlite as nanoseconds since epoch
      // let's do that across the board to avoid mistakes
      [
        this.id,
        this._name,
        this._content,
        this.created.getTime() * 1000000,
        this.updated.getTime() * 1000000,
        this.workspaceId,
        this.legacyWorkspaceId,
        this.source,
        this.sourceInfo,
        this.forkedFrom,
        this.remoteInfo,
        this.viewed_at ? this.viewed_at.getTime() * 1000000 : null,
      ],
    );

    if (this._ydocChanged) {
      await OnlineRunbook.saveYDocForRunbook(this.id, this._ydoc);
      this._ydocChanged = false;
    }

    if (!this.persisted) {
      dbHook("runbook", "create", this);
    } else {
      dbHook("runbook", "update", this);
    }
    this.persisted = true;

    return undefined;
  }

  public async clearRemoteInfo() {
    this.remoteInfo = null;
    await this.save();
  }

  public static async loadYDocForRunbook(id: string): Promise<ArrayBuffer | null> {
    return await invoke<ArrayBuffer | null>("load_ydoc_for_runbook", {
      runbookId: id,
    });
  }

  public static async saveYDocForRunbook(id: string, update: Uint8Array | null) {
    if (update) {
      await logger.time(`Saving Y.Doc for runbook ${id}...`, async () => {
        await invoke("save_ydoc_for_runbook", update, {
          headers: {
            id: id,
          },
        });
      });
    } else {
      logger.info("Skipping serialization of Y.Doc as content is null");
    }
  }

  public async moveTo(targetWorkspace: Workspace) {
    const currentWorkspace = await Workspace.get(this.workspaceId);
    if (!currentWorkspace || !currentWorkspace.canManageRunbooks()) {
      throw new Error("Cannot move runbook out of a workspace without manage_runbooks permissions");
    }

    if (!targetWorkspace || !targetWorkspace.get("id") || !targetWorkspace.canManageRunbooks()) {
      throw new Error("Cannot move runbook to a workspace without manage_runbooks permissions");
    }

    const db = await AtuinDB.load("runbooks");

    await logger.time(`Moving runbook to workspace ${targetWorkspace.get("id")}`, async () => {
      await db.execute(`UPDATE runbooks SET workspace_id = $1 where id = $2`, [
        targetWorkspace.get("id")!,
        this.id,
      ]);
    });

    this.workspaceId = targetWorkspace.get("id")!;

    dbHook("runbook", "update", this);
    // TODO: should this be migrated?? maybe using the shared state nav means it won't matter?
    // dbHook("legacy_workspace", "update", oldWorkspace);
    // dbHook("legacy_workspace", "update", newWorkspace);
  }

  public async updateRemoteInfo(remoteInfo: string | null) {
    const db = await AtuinDB.load("runbooks");

    await db.execute(`UPDATE runbooks SET remote_info = $1 where id = $2`, [remoteInfo, this.id]);
    dbHook("runbook", "update", this);
  }

  public async markViewed() {
    const db = await AtuinDB.load("runbooks");

    await db.execute(`UPDATE runbooks SET viewed_at = $1 where id = $2`, [
      new Date().getTime() * 1000000,
      this.id,
    ]);
    dbHook("runbook", "update", this);
  }

  public clone() {
    return new OnlineRunbook(
      {
        id: this.id,
        name: this.name,
        source: this.source,
        sourceInfo: this.sourceInfo,
        content: this.content,
        _ydoc: this._ydoc,
        created: this.created,
        updated: this.updated,
        workspaceId: this.workspaceId,
        legacyWorkspaceId: this.legacyWorkspaceId,
        forkedFrom: this.forkedFrom,
        remoteInfo: this.remoteInfo,
        viewed_at: this.viewed_at,
      },
      this.persisted,
    );
  }

  public async delete() {
    const db = await AtuinDB.load("runbooks");

    const p1 = db.execute("delete from runbooks where id=$1", [this.id]);
    const p2 = Snapshot.deleteForRunbook(this.id);
    const p3 = invoke<null>("delete_runbook_cleanup", {
      runbook: this.id,
    });
    await Promise.all([p1, p2, p3]);

    dbHook("runbook", "delete", this);
  }
}

function camelCaseToSnakeCase(str: string) {
  return str.replace(/([A-Z])/g, "_$1").toLowerCase();
}

///// OFFLINE

export interface OfflineRunbookAttrs extends BaseRunbookAttrs {
  contentHash: string;
}

export class OfflineRunbook extends Runbook {
  contentHash: string;

  constructor(attrs: OfflineRunbookAttrs, persisted: boolean = false) {
    super(attrs, persisted);
    this.contentHash = attrs.contentHash;
  }

  get viewed_at(): Date | null {
    return new Date();
  }

  public static async allInAllWorkspaces(): Promise<Runbook[]> {
    const ids = await this.allIdsInAllWorkspaces();
    const promises = ids.map((id) => this.load(id));
    const results = await Promise.all(promises);
    return results.filter((result) => result !== null);
  }

  public static async allIdsInAllWorkspaces(): Promise<string[]> {
    const manager = WorkspaceManager.getInstance();
    const workspaces = manager.getWorkspaces();
    return workspaces.flatMap((workspace) => {
      return Object.values(workspace.runbooks).map((runbook) => runbook!.id);
    });
  }

  public static async allFromOrg(orgId: string | null): Promise<Runbook[]> {
    const workspaces = await Workspace.all({ orgId });
    const promises = workspaces.map((workspace) => {
      return OfflineRunbook.allFromWorkspace(workspace.get("id")!);
    });

    const resolved = await Promise.all(promises);
    return resolved.flat();
  }

  public static async allFromWorkspace(workspaceId: string): Promise<Runbook[]> {
    const manager = WorkspaceManager.getInstance();
    const info = manager.getWorkspaceInfo(workspaceId);
    const ids = info
      .andThen((info) => info.ok())
      .map((info) => info.runbooks)
      .map((runbooks) => {
        return Object.values(runbooks).map((runbook) => {
          return runbook!.id;
        });
      })
      .unwrapOr([] as string[]);

    const runbooks = await Promise.allSettled(ids.map((id) => OfflineRunbook.load(id)));
    return runbooks
      .filter((result) => result.status === "fulfilled")
      .map((result) => result.value)
      .filter((result) => result !== null);
  }

  public static async allForkedFrom(runbookId: string): Promise<OfflineRunbook[]> {
    const manager = WorkspaceManager.getInstance();
    const workspaces = manager.getWorkspaces();
    const forkedRunbookIds = workspaces.flatMap((workspace) => {
      return Object.values(workspace.runbooks)
        .filter((runbook) => runbook?.forked_from === runbookId)
        .map((runbook) => runbook!.id);
    });

    const runbooks = await Promise.allSettled(
      forkedRunbookIds.map((id) => OfflineRunbook.load(id)),
    );
    return runbooks
      .filter((result) => result.status === "fulfilled")
      .map((result) => result.value)
      .filter((result) => result !== null);
  }

  public static async count(): Promise<number> {
    const manager = WorkspaceManager.getInstance();
    const workspaces = manager.getWorkspaces();
    return workspaces.reduce((acc, workspace) => {
      return acc + Object.values(workspace.runbooks).length;
    }, 0);
  }

  public static async create(
    workspace: Workspace,
    parentFolderId: string | null,
    persist: boolean = true,
    name: string = "Untitled",
    content: any = [],
    forkedFrom: string | null = null,
  ): Promise<OfflineRunbook | null> {
    if (!persist) {
      throw new Error("Cannot create offline runbook without persisting");
    }

    const idResult = await commands.createRunbook(
      workspace.get("id")!,
      parentFolderId,
      name,
      content,
      forkedFrom,
    );
    if (idResult.isErr()) {
      console.error("Failed to create runbook", idResult.unwrapErr());
      return null;
    }

    const id = idResult.unwrap();

    // We need to wait for the filesystem event to be processed by the backend
    // before we can successfully load the runbook.
    let runbook: OfflineRunbook | null = null;
    let startAttempts = Date.now();
    while (runbook === null) {
      runbook = await OfflineRunbook.load(id);
      if (runbook) {
        return runbook;
      }
      if (Date.now() - startAttempts > 2000) {
        console.error("Failed to load runbook after 2 seconds");
        return null;
      }
      await timeoutPromise(50, null);
    }

    return null;
  }

  public static async load(id: string): Promise<OfflineRunbook | null> {
    const runbook = await commands.getRunbook(id);

    return runbook
      .map(
        (runbook) =>
          new OfflineRunbook({
            id: runbook.id,
            name: runbook.name,
            content: JSON.stringify(runbook.content),
            contentHash: runbook.contentHash,
            workspaceId: runbook.workspaceId,
            created: convertSystemTime(runbook.created) ?? new Date(),
            updated: convertSystemTime(runbook.updated) ?? new Date(),
          }),
      )
      .map((runbook) => {
        return runbook;
      })
      .unwrapOr(null);
  }

  public isOnline(): boolean {
    return false;
  }

  public async save(): Promise<string | undefined> {
    const result = await commands.saveRunbook(
      this.workspaceId,
      this.id,
      this.name,
      JSON.parse(this.content),
    );
    if (result.isErr()) {
      throw result.unwrapErr();
    } else {
      return result.unwrap();
    }
  }

  public async clearRemoteInfo(): Promise<void> {
    // no op
  }

  public async moveTo(_targetWorkspace: Workspace): Promise<void> {
    // TODO
  }

  public async updateRemoteInfo(_remoteInfo: string | null): Promise<void> {
    // no op
  }

  public async markViewed(): Promise<void> {
    // no op
  }

  public clone(): Runbook {
    return new OfflineRunbook({
      id: this.id,
      name: this.name,
      content: this.content,
      contentHash: this.contentHash,
      workspaceId: this.workspaceId,
      created: this.created,
      updated: this.updated,
    });
  }

  public async delete(): Promise<void> {
    const result = await commands.deleteRunbook(this.workspaceId, this.id);
    if (result.isErr()) {
      throw result.unwrapErr();
    } else {
      return;
    }
  }

  get source(): RunbookSource {
    return "local";
  }
}

function convertSystemTime(
  time: { secs_since_epoch: number; nanos_since_epoch: number } | null,
): Date | null {
  if (time === null) {
    return null;
  }
  return new Date(time.secs_since_epoch * 1000 + time.nanos_since_epoch / 1000000);
}
