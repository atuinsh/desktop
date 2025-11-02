import { JsonValue } from "@/rs-bindings/serde_json/JsonValue";

export interface ColumnSchema {
  name: string;
  type: string;
}

export interface TableSchema {
  name: string;
  columns: ColumnSchema[];
}

export interface QueryResult {
  columns?: ColumnSchema[] | null;
  rows?: Record<string, JsonValue>[] | null;

  rowsRead?: number | null;
  bytesRead?: number | null;
  rowsAffected?: number | null;
  lastInsertID?: number | null;

  duration: number;
  time: Date;
}
