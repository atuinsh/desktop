import { create } from "zustand";
import { PtyMetadata } from "@/rs-bindings/PtyMetadata";
import { withoutProperties } from "@/lib/utils";

export interface PtyStore {
  ptys: { [pid: string]: PtyMetadata };

  ptyForBlock: (block: string) => PtyMetadata | null;
  addPty: (pty: PtyMetadata) => void;
  removePty: (pid: string) => void;
}

export const usePtyStore = create<PtyStore>(
  (set, get): PtyStore => ({
    ptys: {},

    ptyForBlock: (block: string): PtyMetadata | null => {
      let ptys = Object.entries(get().ptys)
        .filter(([_, pty]) => pty.block === block)
        .map(([_, pty]) => pty);

      if (ptys.length >= 1) return ptys[ptys.length - 1];

      return null;
    },

    addPty: (pty: PtyMetadata) => {
      set((state: PtyStore) => ({
        ptys: {
          ...state.ptys,
          [pty.pid]: pty,
        },
      }));
    },

    removePty: (pid: string) => {
      set((state: PtyStore) => ({
        ptys: withoutProperties(state.ptys, [pid]),
      }));
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
