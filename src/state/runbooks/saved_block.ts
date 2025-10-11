import { FieldSpecs, GlobalSpec, Model, Persistence } from "ts-tiny-activerecord";
import createTauriAdapter, { setTimestamps } from "@/lib/db/tauri-ar-adapter";
import { DateEncoder, JSONEncoder } from "@/lib/db/encoders";
import { dbHook } from "@/lib/db_hooks";

export type SavedBlockAttrs = {
  id?: string;
  name: string;
  content: string;
  created?: Date;
  updated?: Date;
};

const adapter = createTauriAdapter<SavedBlockAttrs>({
  dbName: "runbooks",
  tableName: "saved_blocks",
});

const fieldSpecs: FieldSpecs<SavedBlockAttrs> = {
  content: { encoder: JSONEncoder },
  created: { encoder: DateEncoder },
  updated: { encoder: DateEncoder },
};

const globalSpec: GlobalSpec<SavedBlockAttrs> = {
  preSave: async (context, model, _type) => {
    if (model.get("name").trim() === "") {
      throw new Error("Name cannot be empty");
    }

    setTimestamps(context, model);
  },
  postSave: async (_context, model, type) => {
    dbHook("saved_block", type === "insert" ? "create" : "update", model);
  },
  postDelete: async (_context, model) => {
    dbHook("saved_block", "delete", model);
  },
};

@Persistence<SavedBlockAttrs>(adapter, fieldSpecs, globalSpec)
export default class SavedBlock extends Model<SavedBlockAttrs> {
  //
}
