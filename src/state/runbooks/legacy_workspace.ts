import { uuidv7 } from "uuidv7";
import Runbook, { OnlineRunbook } from "./runbook";
import AtuinDB from "../atuin_db";
import { dbHook } from "@/lib/db_hooks";

class WorkspaceMeta {
  totalRunbooks: number;

  constructor(totalRunbooks: number) {
    this.totalRunbooks = totalRunbooks;
  }

  static async load(workspace_id: string): Promise<WorkspaceMeta> {
    // Load the meta for the workspace.
    // This is a separate class to keep the workspace class clean.
    const db = await AtuinDB.load("runbooks");

    let runbookCount = await db.select<any[]>(
      "select count(*) as count from runbooks where legacy_workspace_id = ?",
      [workspace_id],
    );

    return new WorkspaceMeta(runbookCount[0].count);
  }
}

export default class Workspace {
  id: string;
  name: string;

  created: Date;
  updated: Date;

  meta: WorkspaceMeta | null;
  watchDir: string | null;

  persisted: boolean = false;

  constructor(
    id: string,
    name: string,
    created: Date,
    updated: Date,
    watchDir: string | null = null,
  ) {
    this.id = id;
    this.name = name;
    this.created = created;
    this.updated = updated;
    this.watchDir = watchDir;

    this.meta = null;
  }

  static build(name: string): Workspace {
    let id = uuidv7();
    return new Workspace(id, name, new Date(), new Date());
  }

  static async create(name: string): Promise<Workspace> {
    let workspace = Workspace.build(name);
    await workspace.save();
    return workspace;
  }

  public async save() {
    const db = await AtuinDB.load("runbooks");
    await db.execute(
      `insert into legacy_workspaces (id, name, created, updated) VALUES ($1, $2, $3, $4)

        on conflict(id) do update
          set
            name = $2,
            created = $3,
            updated = $4`,
      [this.id, this.name, this.created, this.updated],
    );

    if (!this.persisted) {
      dbHook("legacy_workspace", "create", this);
    } else {
      dbHook("legacy_workspace", "update", this);
    }

    this.persisted = true;
  }

  public async isEmpty(): Promise<boolean> {
    // Used to emptiness checks from rust
    return (await this.length()) === 0;
  }

  public async length(): Promise<number> {
    let runbooks = await this.runbooks();
    return runbooks.length;
  }

  async delete() {
    const db = await AtuinDB.load("runbooks");

    // First, delete all runbooks belonging to this workspace.
    let runbooks = await this.runbooks();

    for (let runbook of runbooks) {
      await runbook.delete();
    }

    await db.execute("delete from legacy_workspaces where id = ?", [this.id]);
    dbHook("legacy_workspace", "delete", this);
  }

  static async findById(id: string): Promise<Workspace | null> {
    const db = await AtuinDB.load("runbooks");

    let row = await db.select<any[]>("select * from legacy_workspaces where id = ?", [id]);
    if (row == null) {
      return null;
    }

    const ws = new Workspace(row[0].id, row[0].name, row[0].created, row[0].updated);
    ws.persisted = true;
    return ws;
  }

  static async all(): Promise<Workspace[]> {
    const db = await AtuinDB.load("runbooks");
    let rows = await db.select<any[]>("select * from legacy_workspaces");

    return rows.map((row) => {
      let ws = new Workspace(row.id, row.name, row.created, row.updated);
      ws.persisted = true;

      return ws;
    });
  }

  static async count(): Promise<number> {
    const db = await AtuinDB.load("runbooks");
    let res = await db.select<any[]>("select count(1) as count from legacy_workspaces");

    return res[0]["count"];
  }

  async refreshMeta() {
    let meta = await WorkspaceMeta.load(this.id);
    this.meta = meta;
  }

  async rename(name: string) {
    this.name = name;

    const db = await AtuinDB.load("runbooks");
    await db.execute("update legacy_workspaces set name = ?, updated = ? where id = ?", [
      this.name,
      new Date(),
      this.id,
    ]);

    dbHook("legacy_workspace", "update", this);
  }

  async runbooks(): Promise<Runbook[]> {
    const db = await AtuinDB.load("runbooks");
    let rows = await db.select<any[]>(
      "select * from runbooks where legacy_workspace_id = ? order by updated desc",
      [this.id],
    );
    let runbooks = rows.map((row) => OnlineRunbook.fromRow(row));

    return runbooks;
  }

  async setWatchDir(dir: string) {
    this.watchDir = dir;

    const db = await AtuinDB.load("runbooks");
    await db.execute("update legacy_workspaces set watch_dir = ? where id = ?", [
      this.watchDir,
      this.id,
    ]);
  }
}
