import { cn, usernameFromNwo } from "@/lib/utils";
import { Tooltip } from "@heroui/react";
import { BookLockIcon, BookPlusIcon, BookTextIcon, Terminal } from "lucide-react";
import { NodeRendererProps } from "react-arborist";
import { useStore } from "@/state/store";
import { usePtyStore } from "@/state/ptyStore";
import { useMemo, useRef } from "react";
import { RemoteRunbook } from "@/state/models";
import { OnlineRunbook } from "@/state/runbooks/runbook";
import { TabUri } from "@/state/store/ui_state";
import { useCurrentTabRunbookId } from "@/lib/hooks/useCurrentTab";
import { useQuery } from "@tanstack/react-query";
import { runbookById } from "@/lib/queries/runbooks";
import { remoteRunbook as remoteRunbookQuery } from "@/lib/queries/runbooks";

export interface RunbookRowData {
  type: "runbook";
  id: string;
  name: string;
}

export interface RunbookTreeRowProps extends NodeRendererProps<RunbookRowData> {
  runbookId: string;
  useProvidedName: boolean;
  onContextMenu: (evt: React.MouseEvent<HTMLDivElement>, itemId: string) => void;
}

export default function RunbookTreeRow(props: RunbookTreeRowProps) {
  const ptys = usePtyStore((state) => state.ptys);
  const count = Object.values(ptys).filter((pty) => pty.runbook === props.node.id).length;
  const sidebarClickStyle = useStore((state) => state.sidebarClickStyle);
  const tabs = useStore((state) => state.tabs);
  const currentTabRunbookId = useCurrentTabRunbookId();
  const isActive = currentTabRunbookId === props.node.id;
  const isOpenInAnyTab = useMemo(() => {
    return tabs.some((tab) => {
      const uri = new TabUri(tab.url);
      return uri.isRunbook() && uri.getRunbookId() === props.node.id;
    });
  }, [tabs, props.node.id]);
  const { data: runbook, isLoading: localRunbookLoading } = useQuery(runbookById(props.runbookId));
  const localRunbookName = useMemo(() => {
    return runbook?.name ?? null;
  }, [runbook]);

  // Normally, we get the runbook name from the local runbook.
  // However, if the user has background sync turned off, we don't have a local runbook to pull from.
  // In that case, we get the runbook name from the remote runbook.
  const { data: remoteRunbook } = useQuery({
    ...remoteRunbookQuery(props.runbookId),
    enabled: !localRunbookLoading && !runbook,
  });

  let lastClick = useRef<number>(0);

  function handleClick(evt: React.MouseEvent<HTMLDivElement>) {
    evt.preventDefault();
    evt.stopPropagation();

    if (sidebarClickStyle === "link") {
      if (evt.shiftKey) {
        props.node.selectContiguous();
      } else if (evt.ctrlKey || evt.metaKey) {
        if (props.node.isSelected) {
          props.node.deselect();
        } else {
          props.node.selectMulti();
        }
      } else {
        props.node.select();
        props.node.activate();
      }
    } else {
      if (evt.shiftKey) {
        props.node.selectContiguous();
      } else if (evt.ctrlKey || evt.metaKey) {
        if (props.node.isSelected) {
          props.node.deselect();
        } else {
          props.node.selectMulti();
        }
      } else {
        props.node.select();
      }

      if (lastClick.current) {
        const delta = Date.now() - lastClick.current;
        if (delta < 500) {
          console.log("double click");
          props.node.activate();
        }
      }

      lastClick.current = Date.now();
    }
  }

  async function handleRightClickRunbook(evt: React.MouseEvent<HTMLDivElement>) {
    props.onContextMenu(evt, props.node.data.id);
  }

  let hubRunbookOwnedByUser = false;
  let hubRunbookNotOwnedButHasPermission = false;
  let hubRunbookNotOwnedAndNoPermission = false;
  let RunbookIcon = BookTextIcon;

  if (runbook && runbook.isOnline() && (runbook as OnlineRunbook).remoteInfo) {
    // TODO?
    const remoteInfo: RemoteRunbook = JSON.parse((runbook as OnlineRunbook).remoteInfo || "{}");
    hubRunbookOwnedByUser = usernameFromNwo(remoteInfo.nwo) === useStore.getState().user?.username;
    hubRunbookNotOwnedButHasPermission =
      !hubRunbookOwnedByUser && remoteInfo.permissions.includes("update_content");
    hubRunbookNotOwnedAndNoPermission =
      !hubRunbookOwnedByUser && !hubRunbookNotOwnedButHasPermission;

    if (hubRunbookNotOwnedButHasPermission) {
      RunbookIcon = BookPlusIcon;
    } else if (hubRunbookNotOwnedAndNoPermission) {
      RunbookIcon = BookLockIcon;
    }
  }

  let tooltipContent = "This is a local runbook";
  if (hubRunbookOwnedByUser) {
    tooltipContent = "This runbook has been shared to Atuin Hub";
  } else if (hubRunbookNotOwnedButHasPermission) {
    tooltipContent = "You're collaborating on this runbook";
  } else if (hubRunbookNotOwnedAndNoPermission) {
    tooltipContent = "This runbook belongs to another user";
  }

  return (
    <div
      ref={props.dragHandle}
      style={props.style}
      onClick={handleClick}
      onContextMenu={handleRightClickRunbook}
      className={cn(
        `relative text-ellipsis overflow-hidden hover:bg-gray-100 dark:hover:bg-gray-800 p-[1px]`,
        {
          "cursor-pointer": sidebarClickStyle === "link",
          "bg-blue-200 dark:bg-blue-800 border border-1 border-blue-200 hover:bg-blue-100 hover:dark:bg-blue-900":
            props.node.isSelected,
          "border border-1 border-blue-400": props.node.isSelectedEnd,
          "bg-gray-100 dark:bg-gray-800": isActive || isOpenInAnyTab,
          "bg-blue-200/50 dark:bg-blue-800/50": props.node.isSelected && isActive,
        },
      )}
      id={`${props.node.data.id}-runbook-tree-row`}
    >
      <div className={cn("flex justify-between items-start ml-1", {})}>
        <h3
          className={cn(
            "text-sm overflow-hidden whitespace-nowrap text-ellipsis text-gray-600 dark:text-gray-400",
            {
              "text-foreground dark:text-gray-200": isActive,
            },
          )}
        >
          <span title={tooltipContent}>
            <RunbookIcon
              className={cn("w-4 h-4 mr-1 inline-block", {
                "stroke-green-700 dark:stroke-green-500": hubRunbookOwnedByUser,
                "stroke-blue-600 dark:stroke-blue-400": hubRunbookNotOwnedButHasPermission,
                "stroke-orange-600 dark:stroke-orange-400": hubRunbookNotOwnedAndNoPermission,
              })}
            />
          </span>
          <span
            className={cn("font-normal", {
              "font-semibold": runbook && !runbook.viewed_at,
              "text-gray-900 dark:text-gray-100": runbook && !runbook.viewed_at,
            })}
          >
            {!localRunbookName && !props.useProvidedName && !remoteRunbook && (
              <span className="italic">Loading...</span>
            )}
            {!localRunbookName && !props.useProvidedName && remoteRunbook && (
              <span>{remoteRunbook.name}</span>
            )}
            {props.useProvidedName && <span>{props.node.data.name}</span>}
            {!props.useProvidedName && localRunbookName && (localRunbookName || "Untitled")}
          </span>
        </h3>
        <div className="flex items-center">
          {count > 0 && (
            <Tooltip content={`${count} active terminal${count > 1 ? "s" : ""}`}>
              <div className="flex items-center text-primary-500 ml-1 mr-2 mt-1">
                <Terminal size={14} />
                <span className="text-xs ml-1">{count}</span>
              </div>
            </Tooltip>
          )}
        </div>
      </div>
    </div>
  );
}
