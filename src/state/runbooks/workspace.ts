import { GlobalSpec, Model, Persistence } from "ts-tiny-activerecord";
import createTauriAdapter, { setTimestamps } from "@/lib/db/tauri-ar-adapter";
import { DateEncoder, JSONEncoder } from "@/lib/db/encoders";
import { dbHook } from "@/lib/db_hooks";
import { deleteSharedStateDocument } from "@/lib/shared_state/commands";

export type WorkspaceAttrs = {
  id?: string;
  name: string;
  orgId?: string | null;
  online: 1 | 0;
  folder?: string;
  permissions?: string[];

  created?: Date;
  updated?: Date;
};

const adapter = createTauriAdapter<WorkspaceAttrs>({
  dbName: "runbooks",
  tableName: "workspaces",
});

const fieldSpecs = {
  permissions: { encoder: JSONEncoder },
  created: { encoder: DateEncoder },
  updated: { encoder: DateEncoder },
};

const globalSpecs: GlobalSpec<WorkspaceAttrs> = {
  preSave: async (context, model, _type) => {
    setTimestamps(context, model);
  },
  postSave: async (_context, model, type) => {
    dbHook("workspace", type === "insert" ? "create" : "update", model);
  },
  postDelete: async (_context, model) => {
    dbHook("workspace", "delete", model);
    deleteSharedStateDocument(`workspace-folder:${model.get("id")}`);
  },
};

@Persistence<WorkspaceAttrs>(adapter, fieldSpecs, globalSpecs)
export default class Workspace extends Model<WorkspaceAttrs> {
  static async allOrgIds(): Promise<string[]> {
    const set = new Set<string>();
    const workspaces = await Workspace.all();
    workspaces.forEach((w) => {
      if (w.isOrgOwned()) {
        set.add(w.get("orgId")!);
      }
    });
    return Array.from(set);
  }

  canManageRunbooks(): boolean {
    const permissions = this.get("permissions");
    return !permissions || permissions.includes("manage_runbooks");
  }

  isUserOwned(): boolean {
    return this.get("orgId") === null;
  }

  isOrgOwned(): boolean {
    return this.get("orgId") !== null;
  }

  isLegacyHybrid(): boolean {
    return !this.isOnline() && !this.get("folder");
  }

  isOnline(): boolean {
    return this.get("online") === 1;
  }
}
