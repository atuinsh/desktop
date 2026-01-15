import {
  Button,
  Tooltip,
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
  Progress,
  ButtonGroup,
  CircularProgress,
  Avatar,
  DropdownSection,
  Popover,
  PopoverTrigger,
  PopoverContent,
} from "@heroui/react";
import {
  BookOpenIcon,
  ChartBarBigIcon,
  ChevronDownIcon,
  CircleHelpIcon,
  ExternalLinkIcon,
  HistoryIcon,
  LogOutIcon,
  MailPlusIcon,
  MessageCircleHeartIcon,
  Plus,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  TerminalIcon,
  UsersIcon,
} from "lucide-react";
import { AtuinState, useStore } from "@/state/store";
import { forwardRef, useContext, useEffect, useImperativeHandle, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { PendingInvitations } from "./PendingInvitations";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { SortBy } from "./TreeView";
import { orgWorkspaces, userOwnedWorkspaces } from "@/lib/queries/workspaces";
import { default as WorkspaceComponent } from "./Workspace";
import WorkspaceFolder, { Folder } from "@/state/runbooks/workspace_folders";
import doWorkspaceSetup from "@/lib/workspace_setup";
import RunbookContext from "@/context/runbook_context";
import { createNewRunbookMenu, createRootMenu } from "./menus";
import { SharedStateManager } from "@/lib/shared_state/manager";
import { AtuinSharedStateAdapter } from "@/lib/shared_state/adapter";

import { DndProvider } from "react-dnd";
import { HTML5Backend } from "react-dnd-html5-backend";
import { Rc } from "@binarymuse/ts-stdlib";
import track_event from "@/tracking";
import { open } from "@tauri-apps/plugin-shell";
import AtuinEnv from "@/atuin_env";
import Workspace from "@/state/runbooks/workspace";
import { TabIcon } from "@/state/store/ui_state";
import { clearHubApiToken } from "@/api/auth";
import SocketManager from "@/socket";

const scrollWorkspaceIntoViewGenerator =
  (elRef: React.RefObject<HTMLDivElement | null>) => async (workspaceId: string) => {
    const getElPromise = new Promise<HTMLElement | null>((resolve, reject) => {
      let start = performance.now();
      let el: HTMLElement | null = null;
      const tryGetEl = () => {
        el = document.getElementById(`workspace-el-${workspaceId}`);
        if (el) {
          resolve(el);
        } else if (performance.now() - start < 1000) {
          setTimeout(tryGetEl, 100);
        } else {
          reject(new Error("Element not found"));
        }
      };
      tryGetEl();
    });

    const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    try {
      const el = await getElPromise;
      if (el) {
        // Check if element is completely outside the container
        const rect = el.getBoundingClientRect();
        const containerRect = elRef.current?.getBoundingClientRect();

        if (containerRect) {
          const isCompletelyOutsideViewport =
            rect.bottom <= containerRect.top || // element is entirely above container
            rect.top >= containerRect.bottom || // element is entirely below container
            rect.right <= containerRect.left || // element is entirely to the left of container
            rect.left >= containerRect.right; // element is entirely to the right of container

          // Only scroll if the element is completely outside the container
          if (isCompletelyOutsideViewport) {
            el.scrollIntoView({ behavior: prefersReducedMotion ? "instant" : "smooth" });
          }
        }
      }
    } catch (_e) {
      //
    }
  };

export type ListApi = {
  scrollWorkspaceIntoView: (workspaceId: string) => void;
};

interface NotesSidebarProps {
  onStartCreateRunbook: (workspaceId: string, parentFolderId: string | null) => void;
  onStartCreateWorkspace: () => void;
  moveItemsToWorkspace: (
    items: string[],
    oldWorkspaceId: string,
    newWorkspaceId: string,
    newParentFolderId: string | null,
  ) => void;
  onOpenFeedback: () => void;
  onOpenInvite: () => void;
}

const NoteSidebar = forwardRef((props: NotesSidebarProps, ref: React.ForwardedRef<ListApi>) => {
  const isSyncing = useStore((state: AtuinState) => state.isSyncing);
  const isSearchOpen = useStore((store: AtuinState) => store.searchOpen);
  const setSearchOpen = useStore((store: AtuinState) => store.setSearchOpen);
  const isCommandPaletteOpen = useStore((store: AtuinState) => store.commandPaletteOpen);
  const setCommandPaletteOpen = useStore((store: AtuinState) => store.setCommandPaletteOpen);
  const [sortBy] = useState<SortBy>(SortBy.Name);
  const [pendingWorkspaceMigration, setPendingWorkspaceMigration] = useState<boolean>(true);
  const [focusedWorkspaceId, setFocusedWorkspaceId] = useState<string | null>(null);

  const currentWorkspaceId = useStore((state: AtuinState) => state.currentWorkspaceId);
  const setCurrentWorkspaceId = useStore((state: AtuinState) => state.setCurrentWorkspaceId);
  const openTab = useStore((state: AtuinState) => state.openTab);
  const isLoggedIn = useStore((state: AtuinState) => state.isLoggedIn);
  const refreshUser = useStore((state: AtuinState) => state.refreshUser);

  const elRef = useRef<HTMLDivElement>(null);
  const scrollWorkspaceIntoView = scrollWorkspaceIntoViewGenerator(elRef);
  const user = useStore((state: AtuinState) => state.user);
  const userOrgs = useStore((state: AtuinState) => state.userOrgs);
  const selectedOrg = useStore((state: AtuinState) => state.selectedOrg);
  const setSelectedOrg = useStore((state: AtuinState) => state.setSelectedOrg);

  const { data: workspaces } = useQuery(orgWorkspaces(selectedOrg || null));

  const { activateRunbook, promptDeleteRunbook } = useContext(RunbookContext);

  const queryClient = useQueryClient();

  useImperativeHandle(ref, () => {
    return {
      scrollWorkspaceIntoView,
    };
  });

  useEffect(() => {
    if (useStore.getState().didSidebarSetup) {
      setPendingWorkspaceMigration(false);
      return;
    }

    doWorkspaceSetup().then(() => {
      useStore.setState({ didSidebarSetup: true });
      queryClient.invalidateQueries(userOwnedWorkspaces());
      setPendingWorkspaceMigration(false);
    });
  }, []);

  const handleNewRunbook = async (workspaceId: string, parentFolderId: string | null) => {
    props.onStartCreateRunbook(workspaceId, parentFolderId);
  };

  const handleOpenSearch = async () => {
    if (!isSearchOpen) setSearchOpen(true);
  };

  const handleOpenCommandPalette = async () => {
    if (!isCommandPaletteOpen) setCommandPaletteOpen(true);
  };

  const handleBrowseToOwner = (owner: string) => {
    open(AtuinEnv.url(`/${owner}`));
  };

  const handleCreateOrg = async () => {
    open(AtuinEnv.url(`/${user.username}?tab=orgs`));
  };

  const handleManageOrgMemberships = async () => {
    open(AtuinEnv.url(`/${user.username}?tab=orgs`));
  };

  function handleBrowseToPersonalProfile() {
    handleBrowseToOwner(user.username);
  }

  function handleSelectPersonalOrg() {
    setSelectedOrg(null);
    track_event("org.switch", { to: "personal" });
  }

  function handleNewRunbookInCurrentWorkspace() {
    handleNewRunbook(currentWorkspaceId, null);
  }

  function handleStartDeleteRunbook(_workspaceId: string, runbookId: string) {
    promptDeleteRunbook(runbookId);
  }

  function handleOpenHistory() {
    openTab("/history", "History", TabIcon.HISTORY);
  }

  function handleOpenStats() {
    openTab("/stats", "Stats", TabIcon.STATS);
  }

  function handleOpenSettings() {
    openTab("/settings", "Settings", TabIcon.SETTINGS);
  }

  async function handleLogout() {
    await clearHubApiToken();
    SocketManager.setApiToken(null);
    refreshUser();
  }

  async function handleNewRunbookMenu() {
    if (!workspaces) return;

    const workspaceFolders = await Promise.all(
      workspaces.map(async (ws) => {
        const stateId = `workspace-folder:${ws.get("id")}`;
        const manager = SharedStateManager.getInstance<Folder>(
          stateId,
          new AtuinSharedStateAdapter(stateId),
        );
        const data = await manager.getDataOnce();
        const folder = WorkspaceFolder.fromJS(data);
        Rc.dispose(manager);
        return folder.toArborist();
      }),
    );

    const menu = await createNewRunbookMenu(
      workspaces.map((ws, idx) => ({
        workspace: ws,
        folder: workspaceFolders[idx],
      })),
      {
        onNewRunbook: (workspaceId: string, parentFolderId: string | null) => {
          handleNewRunbook(workspaceId, parentFolderId);
        },
        onNewWorkspace: () => {
          props.onStartCreateWorkspace();
        },
      },
    );

    await menu.popup();
    menu.close();
  }

  async function handleBaseContextMenu(evt: React.MouseEvent<HTMLDivElement>) {
    evt.preventDefault();
    evt.stopPropagation();

    const menu = await createRootMenu({
      onNewWorkspace: () => {
        props.onStartCreateWorkspace();
      },
    });

    await menu.popup();
    menu.close();
  }

  const org = userOrgs.find((org) => org.id === selectedOrg);

  useEffect(() => {
    Workspace.all({ orgId: selectedOrg }).then((workspaces) => {
      if (currentWorkspaceId && !workspaces.some((ws) => ws.get("id") === currentWorkspaceId)) {
        setCurrentWorkspaceId(workspaces[0].get("id")!);
      }
    });
  }, [selectedOrg]);

  return (
    <div
      className={cn([
        "relative h-full sidebar-bg border-r border-gray-200/50 dark:border-zinc-700/50 select-none flex flex-col",
      ])}
      style={{
        width: "100%",
        height: "100%",
      }}
      ref={elRef}
      data-tauri-drag-region
    >
      {pendingWorkspaceMigration && (
        <>
          <div className="p-2 h-[60px] min-height-[60px] flex justify-between items-center border-b border-gray-200 dark:border-gray-600">
            <h2 className="text-lg font-semibold"></h2>
            <div className="flex space-x-1"></div>
          </div>
          <div className="p-2 mt-10 text-center">Setting up...</div>
          <div className="flex justify-center items-center mt-6">
            <CircularProgress />
          </div>
        </>
      )}
      {!pendingWorkspaceMigration && (
        <>
          <Progress
            isIndeterminate={isSyncing}
            size="sm"
            radius="none"
            disableAnimation
            classNames={{ track: "bg-transparent" }}
          />
          <PendingInvitations />
          <div
            className="border-b border-gray-100 dark:border-gray-800 pt-4"
          >
            <Dropdown showArrow size="lg" placement="bottom-end">
              <DropdownTrigger>
                <div
                  className={cn(
                    "flex flex-row justify-between items-center pt-2 pb-3 px-2 pl-3",
                    "hover:bg-gray-100 dark:hover:bg-content2",
                    "cursor-pointer",
                    "border-b border-gray-100 dark:border-gray-800",
                  )}
                >
                  <div className="flex items-center gap-2">
                  {selectedOrg && (
                    <h2 className="text-lg font-semibold flex items-center gap-2">
                      {org?.avatar_url && (
                        <Avatar
                          src={org.avatar_url}
                          size="sm"
                          radius="sm"
                          classNames={{ base: "inline-block mr-2 min-w-[32px]" }}
                          name={org.name}
                        />
                      )}
                      {org?.name}
                    </h2>
                  )}
                  {!selectedOrg && (
                    <h2 className="text-lg font-semibold flex items-center gap-2">
                      {user.isLoggedIn() ? (
                        <>
                          {user.avatar_url && (
                            <Avatar
                              src={user.avatar_url}
                              size="sm"
                              radius="sm"
                              classNames={{ base: "inline-block mr-2 min-w-[32px]" }}
                              name={user.username}
                            />
                          )}
                          {user.username}
                        </>
                      ) : (
                        "Personal"
                      )}
                    </h2>
                  )}
                  </div>
                  <ChevronDownIcon size={16} />
                </div>
              </DropdownTrigger>
              <DropdownMenu selectedKeys={selectedOrg || "personal"}>
                <DropdownSection title="Choose an Organization">
                  <DropdownItem
                    key="personal"
                    startContent={
                      user.avatar_url && (
                        <Avatar
                          src={user.avatar_url}
                          size="sm"
                          radius="sm"
                          classNames={{ base: "inline-block mr-2 min-w-[32px]" }}
                          name={user.username}
                        />
                      )
                    }
                    endContent={
                      <Button
                        isIconOnly
                        size="sm"
                        variant="light"
                        onPress={handleBrowseToPersonalProfile}
                      >
                        <ExternalLinkIcon
                          size={18}
                          className="ml-2 text-gray-500 dark:text-gray-400"
                        />
                      </Button>
                    }
                    onPress={handleSelectPersonalOrg}
                  >
                    {user.isLoggedIn() ? <h3>{user.username} (Personal)</h3> : <h3>Personal</h3>}
                  </DropdownItem>
                  <>
                    {userOrgs.map((org) => (
                      <DropdownItem
                        key={org.id}
                        onPress={() => {
                          setSelectedOrg(org.id);
                          track_event("org.switch", { to: "org" });
                        }}
                        startContent={
                          org.avatar_url && (
                            <Avatar
                              src={org.avatar_url}
                              size="sm"
                              radius="sm"
                              classNames={{ base: "inline-block mr-2 min-w-[32px]" }}
                              name={org.name}
                            />
                          )
                        }
                        endContent={
                          <Button
                            isIconOnly
                            size="sm"
                            variant="light"
                            onPress={() => handleBrowseToOwner(org.slug)}
                          >
                            <ExternalLinkIcon
                              size={18}
                              className="ml-2 text-gray-500 dark:text-gray-400"
                            />
                          </Button>
                        }
                      >
                        {org.name}
                      </DropdownItem>
                    ))}
                  </>
                </DropdownSection>
                {user.isLoggedIn() ? (
                  <DropdownSection title="Management">
                    <DropdownItem
                      key="create-org"
                      onPress={handleCreateOrg}
                      startContent={<PlusIcon size={18} className="mx-1" />}
                    >
                      Create an Organization
                    </DropdownItem>
                    <DropdownItem
                      key="manage-org-memberships"
                      onPress={handleManageOrgMemberships}
                      startContent={<UsersIcon size={18} className="mx-1" />}
                    >
                      Manage Memberships
                    </DropdownItem>
                  </DropdownSection>
                ) : null}
              </DropdownMenu>
            </Dropdown>

            {/* Nav Items */}
            <div className="flex flex-row px-2 pt-3 pb-2 justify-evenly">
              <Tooltip
                content={
                  <span className="flex items-center gap-1.5">
                    Search <kbd className="px-1 py-0.5 text-[10px] bg-gray-600/50 rounded">⌘P</kbd>
                  </span>
                }
                placement="bottom"
                delay={300}
                classNames={{
                  content: "text-xs py-1 px-2 bg-gray-800/90 dark:bg-gray-900/90 text-white rounded shadow-sm",
                }}
              >
                <Button
                  variant="light"
                  size="sm"
                  isIconOnly
                  onPress={handleOpenSearch}
                >
                  <SearchIcon size={18} />
                </Button>
              </Tooltip>
              <Tooltip
                content={
                  <span className="flex items-center gap-1.5">
                    Commands <kbd className="px-1 py-0.5 text-[10px] bg-gray-600/50 rounded">⇧⌘P</kbd>
                  </span>
                }
                placement="bottom"
                delay={300}
                classNames={{
                  content: "text-xs py-1 px-2 bg-gray-800/90 dark:bg-gray-900/90 text-white rounded shadow-sm",
                }}
              >
                <Button
                  variant="light"
                  size="sm"
                  isIconOnly
                  onPress={handleOpenCommandPalette}
                >
                  <TerminalIcon size={18} />
                </Button>
              </Tooltip>
              <Tooltip
                content="History"
                placement="bottom"
                delay={300}
                classNames={{
                  content: "text-xs py-1 px-2 bg-gray-800/90 dark:bg-gray-900/90 text-white rounded shadow-sm",
                }}
              >
                <Button
                  variant="light"
                  size="sm"
                  isIconOnly
                  onPress={handleOpenHistory}
                >
                  <HistoryIcon size={18} />
                </Button>
              </Tooltip>
              <Tooltip
                content="Stats"
                placement="bottom"
                delay={300}
                classNames={{
                  content: "text-xs py-1 px-2 bg-gray-800/90 dark:bg-gray-900/90 text-white rounded shadow-sm",
                }}
              >
                <Button
                  variant="light"
                  size="sm"
                  isIconOnly
                  onPress={handleOpenStats}
                >
                  <ChartBarBigIcon size={18} />
                </Button>
              </Tooltip>
              <Dropdown placement="bottom-end">
                <DropdownTrigger>
                  <Button
                    variant="light"
                    size="sm"
                    isIconOnly
                  >
                    <CircleHelpIcon size={18} />
                  </Button>
                </DropdownTrigger>
                <DropdownMenu
                  aria-label="Help menu"
                  items={[
                    { key: "docs", label: "Documentation", icon: BookOpenIcon, action: () => open("https://docs.atuin.sh/desktop") },
                    { key: "feedback", label: "Send Feedback", icon: MessageCircleHeartIcon, action: props.onOpenFeedback },
                    ...(isLoggedIn() ? [{ key: "invite", label: "Invite Friends", icon: MailPlusIcon, action: props.onOpenInvite }] : []),
                  ]}
                >
                  {(item) => (
                    <DropdownItem
                      key={item.key}
                      startContent={<item.icon size={16} />}
                      onPress={item.action}
                    >
                      {item.label}
                    </DropdownItem>
                  )}
                </DropdownMenu>
              </Dropdown>
              <Tooltip
                content="Settings"
                placement="bottom"
                delay={300}
                classNames={{
                  content: "text-xs py-1 px-2 bg-gray-800/90 dark:bg-gray-900/90 text-white rounded shadow-sm",
                }}
              >
                <Button
                  variant="light"
                  size="sm"
                  isIconOnly
                  onPress={handleOpenSettings}
                >
                  <SettingsIcon size={18} />
                </Button>
              </Tooltip>
            </div>
          </div>

          {/* Workspaces Section */}
          <div className="p-2">
            <ButtonGroup className="w-full">
              <Button
                size="sm"
                variant="flat"
                color="success"
                className="flex-1"
                onPress={handleNewRunbookInCurrentWorkspace}
              >
                <Plus size={18} /> New Runbook
              </Button>
              <Button
                size="sm"
                variant="flat"
                color="success"
                isIconOnly
                onPress={handleNewRunbookMenu}
              >
                <ChevronDownIcon size={18} />
              </Button>
            </ButtonGroup>
          </div>
          <div
            className="p-1 flex-grow overflow-y-auto cursor-default"
            onContextMenu={handleBaseContextMenu}
          >
            <DndProvider backend={HTML5Backend}>
              {workspaces && (
                <div className="flex flex-col gap-2">
                  {workspaces.map((workspace) => {
                    return (
                      <div
                        className="w-[98%] m-auto"
                        onClick={() => setFocusedWorkspaceId(workspace.get("id")!)}
                        key={workspace.get("id")!}
                        id={`workspace-el-${workspace.get("id")!}`}
                      >
                        <WorkspaceComponent
                          workspace={workspace}
                          focused={focusedWorkspaceId === workspace.get("id")}
                          sortBy={sortBy}
                          onActivateRunbook={activateRunbook}
                          onStartCreateRunbook={handleNewRunbook}
                          onStartCreateWorkspace={props.onStartCreateWorkspace}
                          onStartDeleteRunbook={handleStartDeleteRunbook}
                          onStartMoveItemsToWorkspace={props.moveItemsToWorkspace}
                        />
                      </div>
                    );
                  })}
                </div>
              )}
            </DndProvider>
          </div>

          {/* User Info */}
          {user.isLoggedIn() && (
            <div className="border-t border-gray-100 dark:border-gray-800 px-2 py-2">
              <Popover placement="right" offset={10} crossOffset={40}>
                <PopoverTrigger>
                  <div className="flex items-center gap-2 px-1 py-1 text-sm text-gray-500 dark:text-gray-400 cursor-pointer hover:bg-gray-100 dark:hover:bg-content2 rounded">
                    <Avatar
                      src={user.avatar_url || ""}
                      size="sm"
                      radius="full"
                      classNames={{ base: "min-w-[24px] w-6 h-6" }}
                      name={user.username}
                    />
                    <span className="truncate">{user.username}</span>
                  </div>
                </PopoverTrigger>
                <PopoverContent className="p-0">
                  <div className="flex flex-col min-w-[200px]">
                    <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700">
                      <div className="flex flex-col gap-1">
                        <span className="font-medium">{user.username}</span>
                        <span className="text-xs text-gray-500">{user.email}</span>
                        {user.bio && (
                          <span className="text-xs text-gray-400 mt-1">{user.bio}</span>
                        )}
                      </div>
                    </div>
                    <button
                      onClick={handleLogout}
                      className="flex items-center gap-2 px-3 py-2 text-danger hover:bg-danger/10 transition-colors text-left"
                    >
                      <LogOutIcon size={16} />
                      Sign out
                    </button>
                  </div>
                </PopoverContent>
              </Popover>
            </div>
          )}
        </>
      )}
    </div>
  );
});

export default NoteSidebar;
