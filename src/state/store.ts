import { createRunbookState, persistRunbookKeys, AtuinRunbookState } from "./store/runbook_state";
import { createPtyState, persistPtyKeys, AtuinPtyState } from "./store/pty_state";
import { createUiState, persistUiKeys, AtuinUiState } from "./store/ui_state";
import { createUserState, persistUserKeys, AtuinUserState } from "./store/user_state";
import {
  createShellInfoState,
  persistShellInfoKeys,
  AtuinShellInfoState,
} from "./store/shell_info_state";
import {
  AtuinCollaborationState,
  createCollaborationState,
  persistCollaborationKeys,
} from "./store/collaboration_state";

import { create, StateCreator } from "zustand";
import { persist, subscribeWithSelector } from "zustand/middleware";
import Logger from "@/lib/logger";
import { User } from "./models";
import AtuinEnv from "@/atuin_env";
import { AtuinQueryState, createQueryState, persistQueryKeys } from "./store/query_state";
import { createChildState, persistChildKeys } from "./store/child_state";
import { AtuinChildState } from "./store/child_state";
import { AtuinDialogState, createDialogState, persistDialogKeys } from "./store/dialog_state";
import { Settings } from "./settings";
const logger = new Logger("AtuinStore", "purple", "pink");

// To add a new state slice to the store:
// 1. Create a new file in `./store` exporting the following:
//   - A `StateCreator` function that returns the initial state and any actions
//   - An array of keys to persist with Zustand's `persist` middleware
//   - An interface that describes the state
// 2. Add the interface to the `AtuinState` type
// 3. Add the `StateCreator` function to the `create` call
// 4. Add the array of keys to persist to `persistKeys`

// zustand's persist middleware will only trigger a migrate call
// if the previous stored state has a version number
function addVersionToPersistedState() {
  const json = localStorage.getItem("atuin-storage");
  if (json == null) return;

  const persisted = JSON.parse(json);
  if (persisted.version === undefined || persisted.version === null) {
    logger.info("Adding default version (0) to persisted storage");
    persisted.version = 0;
    const newJson = JSON.stringify(persisted);
    localStorage.setItem("atuin-storage", newJson);
  }
}

const persistKeys = [
  persistRunbookKeys,
  persistPtyKeys,
  persistUiKeys,
  persistUserKeys,
  persistShellInfoKeys,
  persistCollaborationKeys,
  persistQueryKeys,
  persistChildKeys,
  persistDialogKeys,
].flat();

export type AtuinState = AtuinPtyState &
  AtuinRunbookState &
  AtuinUiState &
  AtuinUserState &
  AtuinShellInfoState &
  AtuinCollaborationState &
  AtuinChildState &
  AtuinQueryState &
  AtuinDialogState;

const middleware = (f: StateCreator<AtuinState>) =>
  subscribeWithSelector(
    persist(f, {
      name: AtuinEnv.stateStorageName,
      version: 1,

      // don't serialize the terminals map
      // it won't work as JSON. too cyclical
      partialize: (state: AtuinState) =>
        Object.fromEntries(
          Object.entries(state).filter(([key]) => {
            return persistKeys.includes(key as keyof AtuinState);
          }),
        ),

      /**
       * Verisons:
       * 0: initial version
       * 1: Moved currentRunbook to currentRunbookId, dropped runbooks from storage
       */
      migrate: (persisted: any, version) => {
        logger.info(`Migrating state from v${version}`);
        if (version <= 0) {
          const id = persisted.currentRunbook;
          persisted.currentRunbookId = id;
          delete persisted.currentRunbook;
        }

        return persisted;
      },

      onRehydrateStorage: () => {
        return async (state: AtuinState | undefined) => {
          if (!state) return state;

          // In order for cached user data to work correctly, we need to
          // rehydrate the data into an actual User model intance
          const user = state.user as User;
          if (user) {
            const userObj = new User(user.username!, user.email!, user.bio!, user.avatar_url!);
            state.user = userObj;
          }

          // Initialize vim mode from persistent settings if not already set
          const enabled = await Settings.editorVimMode();
          if (state.vimModeEnabled !== enabled) {
            state.setVimModeEnabled(enabled);
          }
        };
      },
    }),
  );

addVersionToPersistedState();
export const useStore = create<AtuinState>()(
  middleware((...a) => ({
    ...createRunbookState(...a),
    ...createPtyState(...a),
    ...createUiState(...a),
    ...createUserState(...a),
    ...createShellInfoState(...a),
    ...createCollaborationState(...a),
    ...createQueryState(...a),
    ...createChildState(...a),
    ...createDialogState(...a),
  })),
);

export type AtuinStore = typeof useStore;
