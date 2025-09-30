import { ColorMode, FunctionalColorMode } from "@/lib/color_modes";
import { Update } from "@tauri-apps/plugin-updater";
import { StateCreator } from "zustand";
import { None, Option, Some } from "@binarymuse/ts-stdlib";
import { uuidv7 } from "uuidv7";

export enum TabIcon {
  HISTORY = "history",
  RUNBOOKS = "runbooks",
  STATS = "stats",
  SETTINGS = "settings",
}

export interface Tab {
  id: string;
  url: string;
  title: string;
  icon: TabIcon | null;
}

export class TabUri {
  constructor(public url: string) {}

  public isRunbook(): boolean {
    return this.url.startsWith("/runbook/");
  }

  public getRunbookId(): string | null {
    if (!this.isRunbook()) {
      return null;
    }
    return this.url.split("/").pop()!;
  }
}

export interface AtuinUiState {
  appVersion: Option<string>;
  focused: boolean;
  connectedToHubSocket: boolean;
  searchOpen: boolean;
  proposedDesktopConnectUser: { username: string; token: string } | undefined;
  isSyncing: boolean;
  colorMode: ColorMode;
  functionalColorMode: FunctionalColorMode;
  fontSize: number;
  fontFamily: string;
  availableUpdate: Update | undefined;
  updating: Option<string>;
  showedUpdatePrompt: boolean;
  // Consumed by React Arborist;
  // { [workspaceId]: { [folderId]: isOpen }}
  folderState: Record<string, Record<string, boolean>>;
  hiddenWorkspaces: Record<string, boolean>;
  sidebarWidth: number;
  sidebarOpen: boolean;
  sidebarClickStyle: "link" | "explorer";
  lastSidebarDragInfo: { itemIds: string[]; sourceWorkspaceId: string } | undefined;
  didSidebarSetup: boolean;

  tabs: Tab[];
  currentTabId: string | null;
  tabCloseHistory: Tab[];
  tabOnClose: Map<string, Set<(tab: Tab) => Promise<boolean>>>;

  lightModeEditorTheme: string;
  darkModeEditorTheme: string;

  setAppVersion: (version: string) => void;
  setFocused: (focused: boolean) => void;
  setConnectedToHubSocket: (online: boolean) => void;
  setSearchOpen: (open: boolean) => void;
  setProposedDesktopConnectuser: (proposedUser?: { username: string; token: string }) => void;
  setIsSyncing: (isSyncing: boolean) => void;
  setColorMode: (colorMode: ColorMode) => void;
  setFunctionalColorMode: (colorMode: FunctionalColorMode) => void;
  setFontSize: (fontSize: number) => void;
  setFontFamily: (fontFamily: string) => void;
  setAvailableUpdate: (update: Update | undefined) => void;
  setUpdating: (updating: Option<string>) => void;
  setShowedUpdatePrompt: (showed: boolean) => void;
  setSidebarWidth: (width: number) => void;
  setSidebarOpen: (open: boolean) => void;
  setSidebarClickStyle: (style: "link" | "explorer") => void;
  setLastSidebarDragInfo: (info?: { itemIds: string[]; sourceWorkspaceId: string }) => void;
  openTab: (url: string, title?: string, icon?: TabIcon) => void;
  closeTab: (url: string) => Promise<boolean>;
  moveTab: (id: string, index: number) => void;
  undoCloseTab: () => void;
  setTabTitle: (id: string, title: string) => void;
  advanceActiveTab: (amount: number) => void;
  closeAllTabs: () => void;
  closeOtherTabs: (id: string) => void;
  closeLeftTabs: (id: string) => void;
  closeRightTabs: (id: string) => void;
  closeTabs: (predicate: (tab: Tab) => boolean) => void;
  registerTabOnClose: (id: string, callback: (tab: Tab) => Promise<boolean>) => void;

  setLightModeEditorTheme: (theme: string) => void;
  setDarkModeEditorTheme: (theme: string) => void;

  getFolderState: (workspaceId: string) => Option<Record<string, boolean>>;
  toggleFolder: (workspaceId: string, folderId: string) => void;
  toggleWorkspaceVisibility: (workspaceId: string) => void;
  // updateFolderState: (workspaceId: string, state: Record<string, boolean>) => void;
  deleteWorkspaceFolderState: (workspaceId: string) => void;
  deleteFolderState: (workspaceId: string, folderId: string) => void;
}

export const persistUiKeys: (keyof AtuinUiState)[] = [
  "colorMode",
  "fontSize",
  "fontFamily",
  "folderState",
  "sidebarWidth",
  "sidebarOpen",
  "sidebarClickStyle",
  "lightModeEditorTheme",
  "darkModeEditorTheme",
  "hiddenWorkspaces",
  "tabs",
  "currentTabId",
];

export const createUiState: StateCreator<AtuinUiState> = (set, get, _store): AtuinUiState => ({
  appVersion: None,
  focused: false,
  connectedToHubSocket: false,
  searchOpen: false,
  proposedDesktopConnectUser: undefined,
  isSyncing: false,
  colorMode: "system",
  functionalColorMode: "light",
  fontSize: 16,
  fontFamily: "Inter",
  availableUpdate: undefined,
  updating: None,
  showedUpdatePrompt: false,
  folderState: {},
  hiddenWorkspaces: {},
  sidebarWidth: 250,
  sidebarOpen: true,
  sidebarClickStyle: "link",
  lastSidebarDragInfo: undefined,
  didSidebarSetup: false,

  tabs: [],
  currentTabId: null,
  tabCloseHistory: [],
  tabOnClose: new Map(),

  lightModeEditorTheme: "githubLight",
  darkModeEditorTheme: "githubDark",

  setAppVersion: (version: string) => set(() => ({ appVersion: Some(version) })),
  setFocused: (focused: boolean) => set(() => ({ focused })),
  setConnectedToHubSocket: (online: boolean) => set(() => ({ connectedToHubSocket: online })),
  setSearchOpen: (open) => set(() => ({ searchOpen: open })),
  setProposedDesktopConnectuser: (proposedUser?) =>
    set(() => ({ proposedDesktopConnectUser: proposedUser })),
  setIsSyncing: (isSyncing: boolean) => set(() => ({ isSyncing })),
  setColorMode: (colorMode: ColorMode) => set(() => ({ colorMode })),
  setFunctionalColorMode: (colorMode: FunctionalColorMode) =>
    set(() => ({ functionalColorMode: colorMode })),
  setFontSize: (fontSize: number) => set(() => ({ fontSize })),
  setFontFamily: (fontFamily: string) => set(() => ({ fontFamily })),
  setAvailableUpdate: (update: Update | undefined) => set(() => ({ availableUpdate: update })),
  setUpdating: (updating: Option<string>) => set(() => ({ updating })),
  setShowedUpdatePrompt: (showed: boolean) => set(() => ({ showedUpdatePrompt: showed })),
  setSidebarWidth: (width: number) => set(() => ({ sidebarWidth: width })),
  setSidebarOpen: (open: boolean) => set(() => ({ sidebarOpen: open })),
  setSidebarClickStyle: (style: "link" | "explorer") => set(() => ({ sidebarClickStyle: style })),
  setLastSidebarDragInfo: (info?: { itemIds: string[]; sourceWorkspaceId: string }) =>
    set(() => ({ lastSidebarDragInfo: info })),
  openTab: (url: string, title?: string, icon?: TabIcon) => {
    const tabs = get().tabs;
    const currentTabId = get().currentTabId;
    const tab = tabs.find((tab) => tab.url === url);
    if (!tab) {
      const currentTabIndex = tabs.findIndex((tab) => tab.id === currentTabId);
      const tabsBefore = tabs.slice(0, currentTabIndex + 1);
      const tabsAfter = tabs.slice(currentTabIndex + 1);
      const id = uuidv7();
      set(() => ({
        tabs: [...tabsBefore, { id, url, title: title || url, icon: icon || null }, ...tabsAfter],
        currentTabId: id,
      }));
    } else {
      set(() => ({ currentTabId: tab.id }));
    }
  },
  closeTab: async (id: string) => {
    const tabs = get().tabs;
    const currentTabId = get().currentTabId;
    const tabToClose = tabs.find((tab) => tab.id === id);

    if (!tabToClose) {
      return false;
    }

    const tabOnClose = get().tabOnClose;
    const callbacks = tabOnClose.get(id) || new Set();
    for (const callback of callbacks) {
      const result = await callback(tabToClose);
      if (!result) {
        return false;
      }
    }

    if (currentTabId !== id) {
      set(() => ({
        tabs: tabs.filter((tab) => tab.id !== id),
        tabCloseHistory: [...get().tabCloseHistory, tabToClose],
      }));
    } else {
      const currentTabIndex = tabs.findIndex((tab) => tab.id === currentTabId);
      let newTabIndex = currentTabIndex;

      const newTabs = tabs.filter((tab) => tab.id !== id);

      if (newTabIndex >= newTabs.length) {
        newTabIndex = newTabs.length - 1;
      }

      set(() => ({
        tabs: newTabs,
        currentTabId: newTabs.length === 0 ? null : newTabs[newTabIndex].id,
        tabCloseHistory: [...get().tabCloseHistory, tabToClose],
      }));
    }

    tabOnClose.delete(id);
    return true;
  },
  moveTab: (id: string, index: number) => {
    const tabs = [...get().tabs];
    const currentIndex = tabs.findIndex((t) => t.id === id);
    if (currentIndex !== -1 && currentIndex !== index) {
      const [movedTab] = tabs.splice(currentIndex, 1);
      tabs.splice(index, 0, movedTab);

      set(() => ({ tabs }));
    }
  },
  undoCloseTab: () => {
    const tabs = get().tabs;
    const tabCloseHistory = [...get().tabCloseHistory];
    let lastClosedTab = tabCloseHistory.pop();
    let tab = tabs.find((tab) => tab.url === lastClosedTab?.url);

    while (tab && tabCloseHistory.length > 0) {
      lastClosedTab = tabCloseHistory.pop();
      tab = tabs.find((tab) => tab.url === lastClosedTab?.url);
    }

    if (tab) {
      set(() => ({
        tabCloseHistory: [],
      }));
    }

    if (lastClosedTab) {
      set(() => ({
        tabs: [...get().tabs, lastClosedTab],
        currentTabId: lastClosedTab.id,
        tabCloseHistory: tabCloseHistory,
      }));
    }
  },
  setTabTitle: (id: string, title: string) => {
    const tabs = get().tabs;
    const tab = tabs.find((tab) => tab.id === id);
    if (tab) {
      tab.title = title;
      set(() => ({ tabs: tabs }));
    }
  },
  advanceActiveTab: (amount: number) => {
    const tabs = get().tabs;
    const currentTabId = get().currentTabId;
    const currentTabIndex = tabs.findIndex((tab) => tab.id === currentTabId);
    let newTabIndex = currentTabIndex + amount;
    if (newTabIndex >= tabs.length) {
      newTabIndex = 0;
    } else if (newTabIndex < 0) {
      newTabIndex = tabs.length - 1;
    }
    set(() => ({ currentTabId: tabs[newTabIndex].id }));
  },
  closeAllTabs: async () => {
    const { tabs, closeTab } = get();
    for (const tab of tabs) {
      await closeTab(tab.id);
    }
  },
  closeOtherTabs: async (id: string) => {
    const { tabs, closeTab } = get();
    const tabsToClose = tabs.filter((tab) => tab.id !== id);
    for (const tab of tabsToClose) {
      await closeTab(tab.id);
    }
  },
  closeLeftTabs: async (id: string) => {
    const { tabs, closeTab } = get();
    const currentTabIndex = tabs.findIndex((tab) => tab.id === id);
    if (currentTabIndex === -1) {
      return;
    }

    const tabsToClose = tabs.filter((_tab, index) => index < currentTabIndex);

    for (const tab of tabsToClose) {
      await closeTab(tab.id);
    }
  },
  closeRightTabs: async (id: string) => {
    const { tabs, closeTab } = get();
    const currentTabIndex = tabs.findIndex((tab) => tab.id === id);
    if (currentTabIndex === -1) {
      return;
    }

    const tabsToClose = tabs.filter((_tab, index) => index > currentTabIndex);

    for (const tab of tabsToClose) {
      await closeTab(tab.id);
    }
  },
  closeTabs: async (predicate: (tab: Tab) => boolean) => {
    const { tabs, closeTab } = get();
    const tabsToClose = tabs.filter(predicate);
    for (const tab of tabsToClose) {
      await closeTab(tab.id);
    }
  },
  registerTabOnClose: (id: string, callback: (tab: Tab) => Promise<boolean>) => {
    const tabOnClose = get().tabOnClose;
    if (!tabOnClose.has(id)) {
      tabOnClose.set(id, new Set());
    }

    const callbacks = tabOnClose.get(id)!;
    callbacks.add(callback);

    return () => {
      callbacks.delete(callback);
    };
  },

  setLightModeEditorTheme: (theme: string) => set(() => ({ lightModeEditorTheme: theme })),
  setDarkModeEditorTheme: (theme: string) => set(() => ({ darkModeEditorTheme: theme })),

  getFolderState: (workspaceId: string) => Some(get().folderState[workspaceId]),
  toggleFolder: (workspaceId: string, folderId: string) => {
    set((state) => {
      const currentState = state.folderState[workspaceId] || {};
      if (currentState[folderId] === undefined) {
        currentState[folderId] = false;
      } else {
        currentState[folderId] = !currentState[folderId];
      }
      return { folderState: { ...state.folderState, [workspaceId]: currentState } };
    });
  },
  toggleWorkspaceVisibility: (workspaceId: string) => {
    set((state) => {
      if (state.hiddenWorkspaces[workspaceId]) {
        delete state.hiddenWorkspaces[workspaceId];
      } else {
        state.hiddenWorkspaces[workspaceId] = true;
      }
      return state;
    });
  },
  // updateFolderState: (workspaceId: string, state: Record<string, boolean>) => {
  //   const currentState = get().folderState[workspaceId] || {};
  //   for (const [folderId, isOpen] of Object.entries(state)) {
  //     currentState[folderId] = isOpen;
  //   }
  //   set(() => ({ folderState: { ...get().folderState, [workspaceId]: currentState } }));
  // },
  deleteWorkspaceFolderState: (workspaceId: string) => {
    const currentState = get().folderState;
    delete currentState[workspaceId];
    set(() => ({ folderState: currentState }));
  },
  deleteFolderState: (workspaceId: string, folderId: string) => {
    const currentState = get().folderState;
    const workspaceState = currentState[workspaceId];
    delete workspaceState[folderId];
    set(() => ({ folderState: { ...currentState, [workspaceId]: workspaceState } }));
  },
});
