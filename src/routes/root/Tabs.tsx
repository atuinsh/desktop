import { cn } from "@/lib/utils";
import { AtuinState, useStore } from "@/state/store";
import { Tab as TabType, TabIcon } from "@/state/store/ui_state";
import {
  BookTextIcon,
  ChartBarBigIcon,
  HistoryIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  SettingsIcon,
  XIcon,
} from "lucide-react";
import React, { useCallback, useEffect, useRef } from "react";
import { useMemo, useState } from "react";
import {
  closestCenter,
  DndContext,
  DragEndEvent,
  DraggableAttributes,
  DragOverlay,
  DragStartEvent,
  MouseSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { SortableContext, horizontalListSortingStrategy, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { SyntheticListenerMap } from "@dnd-kit/core/dist/hooks/utilities";
import { createTabBarMenu, createTabMenu } from "@/components/runbooks/List/menus";
import { Badge } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import debounce from "lodash.debounce";

export const TabsContext = React.createContext<{
  tab: TabType | null;
  setTitle: (title: string) => void;
  setPtyCount: (ptyCount: number) => void;
  incrementBadge: (number: number) => void;
  decrementBadge: (number: number) => void;
  closeTab: () => void;
  reloadTab: () => void;
}>({
  tab: null,
  setTitle: () => {},
  setPtyCount: () => {},
  incrementBadge: () => {},
  decrementBadge: () => {},
  closeTab: () => {},
  reloadTab: () => {},
});

export default function Tabs() {
  // Use individual selectors to avoid creating new object references on every store update
  const tabs = useStore((state: AtuinState) => state.tabs);
  const currentTabId = useStore((state: AtuinState) => state.currentTabId);
  const sidebarOpen = useStore((state: AtuinState) => state.sidebarOpen);
  const setSidebarOpen = useStore((state: AtuinState) => state.setSidebarOpen);
  const openTab = useStore((state: AtuinState) => state.openTab);
  const closeTab = useStore((state: AtuinState) => state.closeTab);
  const setTabTitle = useStore((state: AtuinState) => state.setTabTitle);
  const moveTab = useStore((state: AtuinState) => state.moveTab);
  const closeAllTabs = useStore((state: AtuinState) => state.closeAllTabs);
  const closeOtherTabs = useStore((state: AtuinState) => state.closeOtherTabs);
  const closeLeftTabs = useStore((state: AtuinState) => state.closeLeftTabs);
  const closeRightTabs = useStore((state: AtuinState) => state.closeRightTabs);
  const undoCloseTab = useStore((state: AtuinState) => state.undoCloseTab);
  const setTabPtyCount = useStore((state: AtuinState) => state.setTabPtyCount);
  const incrementTabBadgeCount = useStore((state: AtuinState) => state.incrementTabBadgeCount);
  const decrementTabBadgeCount = useStore((state: AtuinState) => state.decrementTabBadgeCount);

  const updateWindowMenuTabs = useCallback(
    debounce(async (tabs: TabType[]) => {
      invoke<void>("update_window_menu_tabs", { tabs: tabs });
    }, 250),
    [],
  );

  useEffect(() => {
    updateWindowMenuTabs(tabs);
  }, [tabs]);

  const listRef = useRef<HTMLUListElement>(null);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);

  const draggedTab = useMemo(() => {
    return tabs.find((tab) => tab.id === activeDragId);
  }, [activeDragId, tabs]);

  async function handleTabBarContextMenu(e: React.MouseEvent<HTMLUListElement>) {
    e.preventDefault();
    e.stopPropagation();

    const menu = await createTabBarMenu((action) => {
      switch (action.type) {
        case "close_all_tabs":
          closeAllTabs();
          break;
        case "undo_close_tab":
          undoCloseTab();
          break;
        default:
          const x: never = action.type;
          console.error("Unknown action", x);
      }
    });

    await menu.popup();
  }

  const handleWheel = useCallback((event: React.WheelEvent<HTMLUListElement>) => {
    if (event.deltaX > 0) {
      return;
    }

    event.preventDefault();
    if (listRef.current) {
      listRef.current.scrollLeft += event.deltaY;
    }
  }, []);

  function handleDragStart(event: DragStartEvent) {
    const { active } = event;
    setActiveDragId(active.id as string);
  }

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      const index = tabs.findIndex((tab) => tab.id === over.id);
      moveTab(active.id as string, index);
    }

    setActiveDragId(null);
  }

  const sensors = useSensors(
    useSensor(MouseSensor, {
      activationConstraint: {
        distance: 10,
      },
    }),
  );

  function handleToggleSidebar() {
    setSidebarOpen(!sidebarOpen);
  }

  return (
    <div className="flex flex-col h-full bg-background">
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragStart={handleDragStart}
        onDragEnd={handleDragEnd}
      >
        <div
          className={cn(
            "flex flex-row items-center w-full min-h-[40px] border-b overflow-hidden px-2",
            !sidebarOpen && "pl-20"
          )}
          data-tauri-drag-region
        >
          <button
            onClick={handleToggleSidebar}
            className="flex items-center justify-center w-9 h-9 mr-1 rounded hover:bg-gray-100 dark:hover:bg-zinc-800 flex-shrink-0"
            title={sidebarOpen ? "Collapse Sidebar" : "Expand Sidebar"}
          >
            {sidebarOpen ? (
              <PanelLeftCloseIcon size={20} className="text-gray-500" />
            ) : (
              <PanelLeftOpenIcon size={20} className="text-gray-500" />
            )}
          </button>
          <ul
            ref={listRef}
            className="flex flex-row flex-1 overflow-x-auto tab-scrollbar overflow-y-hidden"
            onWheel={handleWheel}
            onContextMenu={handleTabBarContextMenu}
            data-tauri-drag-region
          >
          <SortableContext items={tabs} strategy={horizontalListSortingStrategy}>
            {tabs.map((tab, index) => (
              <Tab
                key={tab.id}
                id={tab.id}
                url={tab.url}
                title={tab.title}
                icon={tab.icon}
                badge={tab.badge}
                index={index}
                active={tab.id === currentTabId}
                onActivate={(url) => openTab(url)}
                onClose={(id) => closeTab(id)}
                onCloseAllTabs={() => closeAllTabs()}
                onCloseOtherTabs={() => closeOtherTabs(tab.id)}
                onCloseLeftTabs={() => closeLeftTabs(tab.id)}
                onCloseRightTabs={() => closeRightTabs(tab.id)}
                onUndoCloseTab={() => undoCloseTab()}
              />
            ))}
          </SortableContext>
          <DragOverlay>
            {draggedTab && (
              <TabDisplay
                id={draggedTab.id + "-drag-overlay"}
                url={draggedTab.url}
                title={draggedTab.title}
                icon={draggedTab.icon}
                badge={draggedTab.badge}
                active={false}
                index={0}
                onActivate={() => {}}
                onClose={() => {}}
                onCloseAllTabs={() => {}}
                onCloseOtherTabs={() => {}}
                onCloseLeftTabs={() => {}}
                onCloseRightTabs={() => {}}
                onUndoCloseTab={() => {}}
                ghost
              />
            )}
          </DragOverlay>
        </ul>
        </div>
      </DndContext>

      {tabs.map((tab) => (
        <TabContextProvider
          key={tab.id}
          tab={tab}
          active={tab.id === currentTabId}
          setTabTitle={setTabTitle}
          setTabPtyCount={setTabPtyCount}
          incrementTabBadgeCount={incrementTabBadgeCount}
          decrementTabBadgeCount={decrementTabBadgeCount}
          closeTab={closeTab}
          openTab={openTab}
        />
      ))}

      {tabs.length === 0 && <div className="flex flex-row w-full"></div>}
    </div>
  );
}

interface TabProps {
  id: string;
  url: string;
  title: string;
  icon: TabIcon | null;
  badge: string | null;
  index: number;
  active: boolean;
  onActivate: (url: string) => void;
  onClose: (id: string) => void;
  onCloseAllTabs: () => void;
  onCloseOtherTabs: () => void;
  onCloseLeftTabs: () => void;
  onCloseRightTabs: () => void;
  onUndoCloseTab: () => void;
}

function Tab(props: TabProps) {
  const { attributes, listeners, setNodeRef, transition, transform } = useSortable({
    id: props.id,
  });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <TabDisplay
      ref={setNodeRef}
      {...props}
      style={style}
      attributes={attributes}
      listeners={listeners}
    />
  );
}

type TabDisplayProps = TabProps & {
  ghost?: boolean;
  style?: React.CSSProperties;
  attributes?: DraggableAttributes;
  listeners?: SyntheticListenerMap;
};

const TabDisplay = React.forwardRef(
  (props: TabDisplayProps, ref: React.ForwardedRef<HTMLLIElement>) => {
    const [hovered, setHovered] = useState(false);
    const elementRef = useRef<HTMLLIElement>(null);
    const combinedRef = (node: HTMLLIElement | null) => {
      elementRef.current = node;
      if (typeof ref === "function") {
        ref(node);
      }
    };

    function handleClick(e: React.MouseEvent<HTMLLIElement>) {
      e.stopPropagation();
      props.onActivate(props.url);
    }

    function handleAuxClick(e: React.MouseEvent<HTMLLIElement>) {
      e.preventDefault();

      if (e.button === 1) {
        props.onClose(props.id);
      } else if (e.button === 2) {
        e.stopPropagation();
        return;
      }
    }

    function handleClose(e: React.MouseEvent<SVGSVGElement>) {
      e.stopPropagation();
      props.onClose(props.id);
    }

    async function handleTabContextMenu(e: React.MouseEvent<HTMLLIElement>) {
      e.preventDefault();
      e.stopPropagation();

      const menu = await createTabMenu((action) => {
        switch (action.type) {
          case "close_tab":
            props.onClose(props.id);
            break;
          case "undo_close_tab":
            props.onUndoCloseTab();
            break;
          case "close_other_tabs":
            props.onCloseOtherTabs();
            break;
          case "close_left_tabs":
            props.onCloseLeftTabs();
            break;
          case "close_right_tabs":
            props.onCloseRightTabs();
            break;
          case "close_all_tabs":
            props.onCloseAllTabs();
            break;
          default:
            const x: never = action.type;
            console.error("Unknown action", x);
        }
      });

      await menu.popup();
    }

    useEffect(() => {
      if (elementRef.current && props.active) {
        elementRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
      }
    }, [props.active]);

    let Icon = null;
    if (props.icon === "history") {
      Icon = HistoryIcon;
    } else if (props.icon === "runbooks") {
      Icon = BookTextIcon;
    } else if (props.icon === "stats") {
      Icon = ChartBarBigIcon;
    } else if (props.icon === "settings") {
      Icon = SettingsIcon;
    }

    return (
      <li
        ref={combinedRef}
        className={cn(
          "flex flex-row gap-2 items-center border-r border-t max-w-[200px] select-none cursor-default p-2 hover:bg-gray-100 dark:hover:bg-zinc-800 hover:cursor-pointer rounded-t-md mt-1 bg-background",
          props.index === 0 && "border-l",
          props.active && "bg-secondary",
          props.ghost && "opacity-50",
          props.ghost && "text-background",
        )}
        onClick={handleClick}
        onAuxClick={handleAuxClick}
        onContextMenu={handleTabContextMenu}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        title={props.title}
        style={props.style}
        {...props.attributes}
        {...props.listeners}
      >
        <Badge color="primary" size="sm" content={props.badge} isInvisible={!props.badge}>
          {Icon && <Icon className="w-4 h-4 min-w-4 min-h-4 inline-block" />}
        </Badge>
        <span
          className="overflow-hidden text-ellipsis whitespace-nowrap"
          style={{ pointerEvents: "none" }}
        >
          {props.title}
        </span>
        <div
          className={cn(
            "opacity-0 hover:bg-gray-200 dark:hover:bg-zinc-700 cursor-pointer rounded-md p-1",
            (props.active || hovered) && "opacity-1",
          )}
        >
          <XIcon className="w-4 h-4 min-w-4 min-h-4" onClick={handleClose} />
        </div>
      </li>
    );
  },
);

interface TabContentProps {
  url: string;
  active: boolean;
}

const LazyHistory = React.lazy(() => import("@/routes/history/History"));
const LazyStats = React.lazy(() => import("@/routes/stats/Stats"));
const LazySettings = React.lazy(() => import("@/components/Settings/Settings"));
const LazyRunbooks = React.lazy(() => import("@/routes/runbooks/Runbooks"));

function TabContent(props: TabContentProps) {
  const router = useMemo(
    () =>
      createMemoryRouter(
        [
          {
            path: "/",
            element: <div>Index</div>,
          },
          {
            path: "/history",
            element: <LazyHistory />,
          },
          {
            path: "/stats",
            element: <LazyStats />,
          },
          {
            path: "/settings",
            element: <LazySettings />,
          },
          {
            path: "/runbook/:runbookId",
            element: <LazyRunbooks />,
          },
        ],
        {
          initialEntries: [props.url],
        },
      ),
    [],
  );

  return (
    <div
      className={cn(props.active && "block", !props.active && "hidden", "h-full overflow-y-auto")}
    >
      <RouterProvider router={router} />
    </div>
  );
}

interface TabContextProviderProps {
  tab: TabType;
  active: boolean;
  setTabTitle: (id: string, title: string) => void;
  setTabPtyCount: (id: string, count: number) => void;
  incrementTabBadgeCount: (id: string, count: number) => void;
  decrementTabBadgeCount: (id: string, count: number) => void;
  closeTab: (id: string) => void;
  openTab: (url: string) => void;
}

function TabContextProvider({
  tab,
  active,
  setTabTitle,
  setTabPtyCount,
  incrementTabBadgeCount,
  decrementTabBadgeCount,
  closeTab,
  openTab,
}: TabContextProviderProps) {
  const setTitle = useCallback(
    (title: string) => setTabTitle(tab.id, title),
    [tab.id, setTabTitle],
  );

  const setPtyCount = useCallback(
    (ptyCount: number) => setTabPtyCount(tab.id, ptyCount),
    [tab.id, setTabPtyCount],
  );

  const incrementBadge = useCallback(
    (number: number = 1) => incrementTabBadgeCount(tab.id, number),
    [tab.id, incrementTabBadgeCount],
  );

  const decrementBadge = useCallback(
    (number: number = 1) => decrementTabBadgeCount(tab.id, number),
    [tab.id, decrementTabBadgeCount],
  );

  const handleCloseTab = useCallback(() => closeTab(tab.id), [tab.id, closeTab]);

  const reloadTab = useCallback(() => {
    const tabUrl = tab.url;
    closeTab(tab.id);
    setTimeout(() => {
      openTab(tabUrl);
    }, 100);
  }, [tab.id, tab.url, closeTab, openTab]);

  const contextValue = useMemo(
    () => ({
      tab,
      setTitle,
      setPtyCount,
      incrementBadge,
      decrementBadge,
      closeTab: handleCloseTab,
      reloadTab,
    }),
    [tab, setTitle, setPtyCount, incrementBadge, decrementBadge, handleCloseTab, reloadTab],
  );

  return (
    <TabsContext.Provider value={contextValue}>
      <TabContent url={tab.url} active={active} />
    </TabsContext.Provider>
  );
}
