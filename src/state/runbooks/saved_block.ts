import { FieldSpecs, GlobalSpec, Model, Persistence } from "ts-tiny-activerecord";
import createTauriAdapter, { setTimestamps } from "@/lib/db/tauri-ar-adapter";
import { DateEncoder, JSONEncoder } from "@/lib/db/encoders";

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
  preSave: setTimestamps,
};

@Persistence<SavedBlockAttrs>(adapter, fieldSpecs, globalSpec)
export default class SavedBlock extends Model<SavedBlockAttrs> {
  //
}
