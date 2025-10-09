import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { create } from "zustand";
import Logger from "@/lib/logger";
const logger = new Logger("PtyStore", "purple", "pink");

export interface PtyMetadata {
  pid: string;
  runbook: string;
  block: string;
  created_at: number;
}

const PTY_OPEN_CHANNEL = "pty_open";
const PTY_KILL_CHANNEL = "pty_kill";

export interface PtyStore {
  ptys: { [pid: string]: PtyMetadata };
  unlistenOpen: UnlistenFn | null;
  unlistenKill: UnlistenFn | null;

  listenBackend: () => Promise<void>;
  unlistenBackend: () => void;
  createPty: (cwd: string, env: any, runbook: string, block: string, shell?: string) => void;
  ptyForBlock: (block: string) => PtyMetadata | null;
}

export const usePtyStore = create<PtyStore>(
  (set, get): PtyStore => ({
    ptys: {},
    unlistenOpen: null,
    unlistenKill: null,

    listenBackend: async () => {
      let unlistenOpen = await listen(PTY_OPEN_CHANNEL, (event) => {
        let data = event.payload as PtyMetadata;

        set((state: PtyStore) => ({
          ptys: {
            ...state.ptys,
            [data.pid]: data,
          },
        }));
      });

      let unlistenKill = await listen(PTY_KILL_CHANNEL, (event) => {
        let data = event.payload as PtyMetadata;

        set((state: PtyStore) => {
          let newPtys = Object.fromEntries(
            Object.entries(state.ptys).filter(([pid, _]) => pid !== data.pid),
          );

          return {
            ptys: newPtys,
          };
        });
      });

      // we also need the intial state :D
      let ptys: PtyMetadata[] = await invoke("pty_list", {});

      logger.debug("ptyState fetched initial pty state and listening for changes");

      set((_state: PtyStore) => ({
        unlistenOpen,
        unlistenKill,
        ptys: ptys.reduce((acc: any, pty: PtyMetadata) => {
          return { ...acc, [pty.pid]: pty };
        }, {}),
      }));
    },

    unlistenBackend: () => {
      set((state: PtyStore) => {
        if (state.unlistenOpen) {
          state.unlistenOpen();
        }
        if (state.unlistenKill) {
          state.unlistenKill();
        }

        return {
          unlistenOpen: null,
          unlistenKill: null,
        };
      });
    },

    createPty: async (cwd: string, env: any, runbook: string, block: string, shell?: string) => {
    let pid = await invoke("pty_open", { cwd, env, runbook, block, shell });

    return pid as string;
    },

    ptyForBlock: (block: string): PtyMetadata | null => {
      let ptys = Object.entries(get().ptys)
        .filter(([_, pty]) => pty.block === block)
        .map(([_, pty]) => pty);

      if (ptys.length >= 1) return ptys[ptys.length - 1];

      return null;
    },
  }),
);

export const ptyForRunbook = (runbook: string): PtyMetadata[] => {
  let all = usePtyStore.getState().ptys;
  let ptys = Object.entries(all)
    .filter(([_, pty]) => pty.runbook === runbook)
    .map(([_, pty]) => pty);

  return ptys;
};
