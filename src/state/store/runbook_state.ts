import Runbook from "../runbooks/runbook";
import { open } from "@tauri-apps/plugin-dialog";
import Logger from "@/lib/logger";
const logger = new Logger("RunbookStore", "purple", "pink");

import { StateCreator } from "zustand";

export interface AtuinRunbookState {
  runbooks: Runbook[];

  // The ID of the runbook that is currently being executed
  // Right now we do not support concurrent or background runbook execution
  serialExecution: string[];
  lastTagForRunbook: { [key: string]: string };
  backgroundSync: boolean;
  syncConcurrency: number;

  importRunbooks: () => Promise<string[]>;
  refreshRunbooks: () => Promise<void>;

  startSerialExecution: (id: string) => void;
  stopSerialExecution: (id: string) => void;
  selectTag: (runbookId: string, tag: string | null) => void;
  getLastTagForRunbook: (runbookId: string) => string | null;

  currentWorkspaceId: string;
  setCurrentWorkspaceId: (id: string) => void;

  setBackgroundSync: (backgroundSync: boolean) => void;
  setSyncConcurrency: (syncConcurrency: number) => void;
}

export const persistRunbookKeys: (keyof AtuinRunbookState)[] = [
  "lastTagForRunbook",
  "currentWorkspaceId",
  "backgroundSync",
  "syncConcurrency",
];

export const createRunbookState: StateCreator<AtuinRunbookState> = (
  set,
  get,
  _store,
): AtuinRunbookState => ({
  runbooks: [],
  lastTagForRunbook: {},
  serialExecution: [],
  backgroundSync: true,
  syncConcurrency: 1,

  importRunbooks: async (): Promise<string[]> => {
    let filePath = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Atuin Runbooks",
          extensions: ["atrb"],
        },
      ],
    });

    if (!filePath || filePath.length === 0) return [];

    return filePath;
  },

  // PERF: Yeah this won't work long term. Let's see how many runbooks people have in reality and optimize for 10x that
  refreshRunbooks: async () => {
    logger.debug("loading runbooks for WS", get().currentWorkspaceId);

    let runbooks = await Runbook.allFromWorkspace(get().currentWorkspaceId!);

    set({ runbooks });
  },

  selectTag: (runbookId: string, tag: string | null) => {
    const obj = get().lastTagForRunbook;
    if (tag) {
      obj[runbookId] = tag;
    } else {
      delete obj[runbookId];
    }
    set({ lastTagForRunbook: obj });
  },

  getLastTagForRunbook: (runbookId?: string): string | null => {
    if (runbookId) {
      return get().lastTagForRunbook[runbookId] || null;
    } else {
      return null;
    }
  },

  currentWorkspaceId: "",

  startSerialExecution: (id: string) => {
    if (get().serialExecution.includes(id)) {
      return;
    }

    set({ serialExecution: [...get().serialExecution, id] });
  },

  stopSerialExecution: (id: string) => {
    set({ serialExecution: get().serialExecution.filter((sid) => sid !== id) });
  },

  setCurrentWorkspaceId: (id: string) => {
    set({ currentWorkspaceId: id });
  },

  setBackgroundSync: (backgroundSync: boolean) => {
    set({ backgroundSync });
  },

  setSyncConcurrency: (syncConcurrency: number) => {
    set({ syncConcurrency });
  },
});
