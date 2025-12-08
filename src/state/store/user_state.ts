import { me } from "@/api/api";
import { DefaultUser, User, UserOrg } from "../models";
import { StateCreator } from "zustand";
import semver from "semver";
import { None, Option } from "@/lib/utils";

export enum ConnectionState {
  LoggedOut,
  Online,
  Offline,
  OutOfDate,
}

function isOutOfDate(currentVersion: string, minimumVersion: string) {
  if (!semver.valid(currentVersion)) return false;
  if (!semver.valid(minimumVersion)) return false;
  return semver.lt(currentVersion, minimumVersion);
}

function calculateConnectionState(
  user: User,
  online: boolean,
  minimumVersion: Option<string>,
  currentVersion: string,
) {
  const outOfDate = minimumVersion
    .map((min) => {
      return isOutOfDate(currentVersion, min);
    })
    .unwrapOr(false);

  if (!online) return ConnectionState.Offline;
  if (outOfDate) return ConnectionState.OutOfDate;
  if (!user.isLoggedIn()) return ConnectionState.LoggedOut;

  return ConnectionState.Online;
}

export interface AtuinUserState {
  user: User;
  userOrgs: UserOrg[];
  selectedOrg: string | null;
  online: boolean;
  currentVersion: string;
  minimumVersion: Option<string>;
  connectionState: ConnectionState;

  isLoggedIn: () => boolean;
  refreshUser: () => Promise<void>;
  setSelectedOrg: (orgId: string | null) => void;

  setOnline: (online: boolean) => void;
  setCurrentVersion: (version: string) => void;
  setMinimumVersion: (version: Option<string>) => void;
  updateConnectionState: () => void;
}

export const persistUserKeys: (keyof AtuinUserState)[] = ["user", "userOrgs", "selectedOrg"];

export const createUserState: StateCreator<AtuinUserState> = (
  set,
  get,
  _store,
): AtuinUserState => ({
  user: DefaultUser,
  userOrgs: [],
  selectedOrg: null,
  online: false,
  currentVersion: "",
  minimumVersion: None,
  connectionState: ConnectionState.Offline,

  isLoggedIn: () => {
    if (!get().user) return false;
    let user = get().user;

    if (!user.isLoggedIn) return false;

    return user.isLoggedIn();
  },
  refreshUser: async () => {
    try {
      let resp = await me();

      if (!resp) {
        set({ user: DefaultUser, userOrgs: [], selectedOrg: null });
        return;
      }

      const { user, orgs } = resp;

      const { selectedOrg } = get();
      let newSelectedOrg = selectedOrg;
      if (!orgs.some((org) => org.id === selectedOrg)) {
        newSelectedOrg = null;
      }

      const oldUser = get().user;
      const newUser = new User(user.username, user.email, "", user.avatar_url);

      if (!oldUser.isEqual(newUser)) {
        set({ user: newUser });
      }

      set({
        userOrgs: orgs,
        selectedOrg: newSelectedOrg,
      });
    } catch (err: any) {
      if (!err.code) {
        // This was due to a network error, don't clear cached user
      } else {
        set({ user: DefaultUser });
      }
    }
  },
  setSelectedOrg: (orgId: string | null) => {
    const orgs = get().userOrgs;
    if (orgId === null || orgs.some((org) => org.id === orgId)) {
      set(() => ({ selectedOrg: orgId }));
    } else {
      set(() => ({ selectedOrg: null }));
    }
  },

  setOnline: (online: boolean) => {
    set(() => ({ online }));
    get().updateConnectionState();
  },
  setCurrentVersion: (version: string) => {
    set(() => ({ currentVersion: version }));
    get().updateConnectionState();
  },
  setMinimumVersion: (version: Option<string>) => {
    set(() => ({ minimumVersion: version }));
    get().updateConnectionState();
  },
  updateConnectionState: () => {
    const user = get().user;
    const online = get().online;
    const currentVersion = get().currentVersion;
    const minimumVersion = get().minimumVersion;

    set(() => ({
      connectionState: calculateConnectionState(user, online, minimumVersion, currentVersion),
    }));
  },
});
