import Runbook from "@/state/runbooks/runbook";
import RelativeTime from "@/components/relative_time.tsx";
import TagSelector from "./TagSelector";
import ColorAvatar from "@/components/ColorAvatar";
import { DateTime } from "luxon";
import { addToast, Avatar, AvatarGroup, Button, Tooltip } from "@heroui/react";
import { RemoteRunbook } from "@/state/models";
import {
  BookTextIcon,
  CopyIcon,
  PencilOffIcon,
  RefreshCcwIcon,
  SettingsIcon,
  SparklesIcon,
  TrashIcon,
} from "lucide-react";
import { PresenceUserInfo } from "@/lib/phoenix_provider";
import { useQuery } from "@tanstack/react-query";
import { workspaceById } from "@/lib/queries/workspaces";
import track_event from "@/tracking";
import PlayButton from "@/lib/blocks/common/PlayButton";
import AtuinEnv from "@/atuin_env";
import { open } from "@tauri-apps/plugin-shell";
import { cn } from "@/lib/utils";
import { resetRunbookState } from "@/lib/runtime";
import { useSerialExecution } from "@/lib/hooks/useSerialExecution";
import { useEffect, useRef } from "react";
import { DialogBuilder } from "@/components/Dialogs/dialog";

type TopbarProps = {
  runbook: Runbook;
  remoteRunbook?: RemoteRunbook;
  tags: Array<{ text: string; value: string }>;
  presences: PresenceUserInfo[];
  currentTag: string | null;
  showTagMenu: boolean;
  canEditTags: boolean;
  canInviteCollaborators: boolean;
  onSelectTag: (tag: string | null) => void;
  onCreateTag: (tag: string) => Promise<void>;
  onDeleteTag: (tag: string) => Promise<void>;
  onOpenTagMenu: () => void;
  onCloseTagMenu: () => void;
  onShareToHub: () => void;
  onDeleteFromHub: () => void;
  onToggleSettings: () => void;
  isSettingsOpen: boolean;
  isAIFeaturesEnabled: boolean;
  isAIAssistantOpen: boolean;
  toggleAIAssistant: () => void;
};

function openHubRunbook(e: React.MouseEvent<HTMLAnchorElement>) {
  e.preventDefault();
  open(e.currentTarget.href);
}

export default function Topbar(props: TopbarProps) {
  let runbook = props.runbook;
  let remoteRunbook = props.remoteRunbook;
  let { data: workspace } = useQuery(workspaceById(runbook.workspaceId));

  const serialExecution = useSerialExecution(runbook.id);

  let name: string;
  if (remoteRunbook) {
    name = remoteRunbook.nwo;
  } else {
    name = runbook.name;
  }

  let wasRunning = useRef(false);
  useEffect(() => {
    if (serialExecution.isRunning && wasRunning.current === false) {
      wasRunning.current = true;
    } else if (!serialExecution.isRunning && wasRunning.current === true) {
      wasRunning.current = false;

      if (serialExecution.isSuccess) {
        addToast({
          title: "Serial execution completed",
          description: `Runbook "${name}" completed successfully`,
          color: "success",
          timeout: 5000,
          shouldShowTimeoutProgress: true,
        });
      } else if (serialExecution.isError) {
        addToast({
          title: "Serial execution failed",
          description: `Runbook "${name}" failed to complete`,
          color: "danger",
          timeout: 5000,
          shouldShowTimeoutProgress: true,
        });
      } else if (serialExecution.isCancelled) {
        addToast({
          title: "Serial execution cancelled",
          description: `Runbook "${name}" was cancelled`,
          color: "warning",
          timeout: 5000,
          shouldShowTimeoutProgress: true,
        });
      }
    }
  }, [
    serialExecution.isRunning,
    serialExecution.isSuccess,
    serialExecution.isError,
    serialExecution.isCancelled,
    serialExecution.error,
  ]);

  function onSelectTag(tag: string) {
    props.onSelectTag(tag);
  }

  function handleDeleteTag() {
    if (!props.currentTag) {
      return;
    }

    props.onDeleteTag(props.currentTag);
  }

  function handleCopyRunbookUrl() {
    if (!remoteRunbook) return;
    navigator.clipboard.writeText(AtuinEnv.url(remoteRunbook.nwo));
    addToast({
      title: "Runbook URL copied to clipboard",
      color: "success",
      radius: "sm",
      timeout: 2000,
      shouldShowTimeoutProgress: false,
    });
  }

  function handleResetRunbookState() {
    resetRunbookState(props.runbook.id);
  }

  function handleStartSerialExecution() {
    track_event("runbooks.serial.execute");
    serialExecution.start();
  }

  function handleStopSerialExecution() {
    serialExecution.stop();
  }

  function toggleAIAssistant() {
    if (props.isAIFeaturesEnabled) {
      props.toggleAIAssistant();
    } else {
      new DialogBuilder()
        .title("AI Features Not Enabled")
        .message("AI features are disabled. Enable in application settings.")
        .action({ label: "OK", value: "ok", variant: "flat" })
        .build();
    }
  }

  const renderBarContents = () => {
    let owner = remoteRunbook?.owner;

    if (!owner && (remoteRunbook as any)?.user) {
      owner = {
        type: "user",
        user: (remoteRunbook as any).user,
      };
    }

    return (
      <div id="topbar" className="flex h-full w-full justify-between pr-4">
        <div id="avatar-name" className="flex min-w-0 md:max-w-full md:grow mr-2">
          {/* TODO: use org avatar for orgs once we have them */}
          {remoteRunbook && owner?.type === "user" && (
            <Avatar
              size="sm"
              radius="sm"
              name={owner.user.username}
              src={owner.user.avatar_url}
              classNames={{ base: "inline-block mr-2 mt-1 min-w-[32px]" }}
            />
          )}
          {remoteRunbook && owner?.type === "org" && (
            <Avatar
              size="sm"
              radius="sm"
              name={owner.org.name}
              src={owner.org.avatar_url || undefined}
              classNames={{ base: "inline-block mr-2 mt-1 min-w-[32px]" }}
            />
          )}
          {!remoteRunbook && <BookTextIcon size={24} className="mt-2 mr-2 ml-1 min-w-[26px]" />}
          <div className="flex-col truncate shrink">
            <div className="hidden md:flex mb-[-1px] whitespace-nowrap md:flex-row items-center">
              {remoteRunbook ? (
                <a href={AtuinEnv.url(remoteRunbook.nwo)} onClick={openHubRunbook}>
                  {name}
                </a>
              ) : (
                name
              )}
              {remoteRunbook && (
                <Tooltip content="Copy runbook URL" placement="bottom" showArrow>
                  <CopyIcon
                    size={16}
                    className="ml-2 cursor-pointer"
                    onClick={handleCopyRunbookUrl}
                  />
                </Tooltip>
              )}
            </div>
            <div className="hidden md:block text-gray-400 text-xs italic whitespace-nowrap">
              Updated <RelativeTime time={DateTime.fromJSDate(runbook.updated)} />
            </div>
          </div>
          <div className="mt-[7px] inline-block">
            {props.runbook.isOnline() && (props.tags.length > 0 || props.canEditTags) && (
              <TagSelector
                runbookId={runbook.id}
                isOpen={props.showTagMenu}
                onTrigger={props.onOpenTagMenu}
                onClose={props.onCloseTagMenu}
                tags={props.tags}
                currentTag={props.currentTag}
                canEditTags={props.canEditTags}
                onSelectTag={onSelectTag}
                onCreateTag={props.onCreateTag}
              />
            )}
          </div>
          {props.currentTag && props.currentTag !== "latest" && (
            <>
              <div className="mx-2">
                <Tooltip content="Delete this tag" placement="bottom" showArrow>
                  <Button isIconOnly variant="faded" color="danger" onPress={handleDeleteTag}>
                    <TrashIcon className="h-4 w-4 text-red-600" />
                  </Button>
                </Tooltip>
              </div>
              <div className="mt-[7px] basis-4 inline-block">
                <Tooltip
                  content="This runbook is in read-only mode because you are viewing a tag"
                  placement="bottom"
                  showArrow
                >
                  <PencilOffIcon className="h-4 w-4 text-yellow-600 ml-4 mr-4 inline" />
                </Tooltip>
              </div>
            </>
          )}
        </div>
        <div className="basis-[100px] shrink-0 flex">
          <AvatarGroup
            size="sm"
            max={5}
            total={props.presences.length}
            classNames={{ base: "hidden lg:inline-block mr-2 mt-1" }}
            renderCount={(count) => {
              if (count - 5 > 0) {
                return <span className="ml-2 text-gray-500">+{count - 5} others</span>;
              } else {
                return <span />;
              }
            }}
          >
            {props.presences.map((user) => (
              <ColorAvatar
                key={user.id}
                isBordered
                size="sm"
                name={user.username}
                src={user.avatar_url}
                imgProps={{ title: user.username }}
                outlineColor={user.color}
                classNames={{ base: "inline-block mb-2" }}
              />
            ))}
          </AvatarGroup>
        </div>
        <Tooltip content="Reset any runbook state changes from block executions" placement="bottom">
          <Button
            isIconOnly
            variant="flat"
            size="sm"
            className="mt-1 ml-2"
            onPress={handleResetRunbookState}
          >
            <RefreshCcwIcon className="h-4 w-4" />
          </Button>
        </Tooltip>
        <Tooltip content="Toggle runbook settings" placement="bottom">
          <Button
            isIconOnly
            variant="flat"
            size="sm"
            className={cn("mt-1 ml-2", props.isSettingsOpen && "bg-gray-400 dark:bg-gray-400")}
            onPress={props.onToggleSettings}
          >
            <SettingsIcon
              className={cn("h-4 w-4", props.isSettingsOpen && "stroke-white dark:stroke-gray-800")}
            />
          </Button>
        </Tooltip>
        <Tooltip content="Toggle AI assistant" placement="bottom">
          <Button
            isIconOnly
            variant="flat"
            size="sm"
            className={cn(
              "mt-1 ml-2",
              props.isAIAssistantOpen && "bg-purple-400 dark:bg-purple-400",
            )}
            onPress={toggleAIAssistant}
          >
            <SparklesIcon
              className={cn(
                "h-4 w-4",
                !props.isAIAssistantOpen && "stroke-purple-600 dark:stroke-purple-400",
                props.isAIAssistantOpen && "stroke-white dark:stroke-gray-800",
              )}
            />
          </Button>
        </Tooltip>
        <PlayButton
          className="mt-1 ml-2"
          isRunning={serialExecution.isRunning}
          cancellable={true}
          tooltip="Run this runbook in serial mode (top-to-bottom)"
          tooltipPlacement="bottom"
          onPlay={handleStartSerialExecution}
          onStop={handleStopSerialExecution}
        />
      </div>
    );
  };

  return (
    <div className="flex w-full max-w-full overflow-hidden bg-gray-50 dark:bg-content1 h-[60px] min-h-[60px] flex-row border-b dark:border-default-300 px-2 justify-between pt-2">
      {workspace && renderBarContents()}
    </div>
  );
}
