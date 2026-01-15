import Runbook from "@/state/runbooks/runbook";
import RelativeTime from "@/components/relative_time.tsx";
import TagSelector from "./TagSelector";
import ColorAvatar from "@/components/ColorAvatar";
import { DateTime } from "luxon";
import { addToast, Avatar, AvatarGroup, Button, ButtonGroup, Tooltip } from "@heroui/react";
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

  const renderBarContents = () => {
    let owner = remoteRunbook?.owner;

    if (!owner && (remoteRunbook as any)?.user) {
      owner = {
        type: "user",
        user: (remoteRunbook as any).user,
      };
    }

    return (
      <div id="topbar" className="flex h-full w-full items-center justify-between pr-4">
        {/* Left section: Avatar + Content */}
        <div id="avatar-name" className="flex min-w-0 flex-1 mr-4">
          {/* Avatar */}
          {remoteRunbook && owner?.type === "user" && (
            <Avatar
              size="sm"
              radius="sm"
              name={owner.user.username}
              src={owner.user.avatar_url}
              classNames={{ base: "inline-block mr-2 min-w-[32px] shrink-0" }}
            />
          )}
          {remoteRunbook && owner?.type === "org" && (
            <Avatar
              size="sm"
              radius="sm"
              name={owner.org.name}
              src={owner.org.avatar_url || undefined}
              classNames={{ base: "inline-block mr-2 min-w-[32px] shrink-0" }}
            />
          )}
          {!remoteRunbook && <BookTextIcon size={24} className="mr-2 ml-1 min-w-[26px] shrink-0" />}

          {/* Two-row content area */}
          <div className="flex flex-col min-w-0 flex-1">
            {/* Row 1: Name/URL + Copy */}
            <div className="hidden md:flex items-center whitespace-nowrap">
              {remoteRunbook ? (
                <a href={AtuinEnv.url(remoteRunbook.nwo)} onClick={openHubRunbook} className="truncate">
                  {name}
                </a>
              ) : (
                <span className="truncate">{name}</span>
              )}
              {remoteRunbook && (
                <Tooltip content="Copy runbook URL" placement="bottom" showArrow>
                  <CopyIcon
                    size={14}
                    className="ml-2 cursor-pointer text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 shrink-0"
                    onClick={handleCopyRunbookUrl}
                  />
                </Tooltip>
              )}
            </div>

            {/* Row 2: Updated time + Tag selector */}
            <div className="hidden md:flex items-center">
              <span className="text-gray-400 text-xs italic whitespace-nowrap">
                Updated <RelativeTime time={DateTime.fromJSDate(runbook.updated)} />
              </span>

              {/* Tag selector + related controls - positioned near content */}
              <div className="flex items-center ml-3">
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
                {props.currentTag && props.currentTag !== "latest" && (
                  <>
                    <Tooltip content="Delete this tag" placement="bottom" showArrow>
                      <Button
                        isIconOnly
                        variant="light"
                        size="sm"
                        className="ml-1 text-red-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950/30"
                        onPress={handleDeleteTag}
                      >
                        <TrashIcon className="h-3.5 w-3.5" />
                      </Button>
                    </Tooltip>
                    <Tooltip
                      content="This runbook is in read-only mode because you are viewing a tag"
                      placement="bottom"
                      showArrow
                    >
                      <PencilOffIcon className="h-3.5 w-3.5 text-yellow-600 ml-2" />
                    </Tooltip>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Middle section: Presence avatars */}
        <div className="shrink-0 flex mr-2">
          <AvatarGroup
            size="sm"
            max={5}
            total={props.presences.length}
            classNames={{ base: "hidden lg:flex" }}
            renderCount={(count) => {
              if (count - 5 > 0) {
                return <span className="ml-2 text-gray-500 text-xs">+{count - 5}</span>;
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
              />
            ))}
          </AvatarGroup>
        </div>

        {/* Right section: Action buttons */}
        <ButtonGroup size="sm" className="shrink-0">
          <Tooltip content="Reset runbook state" placement="bottom">
            <Button
              isIconOnly
              variant="flat"
              size="sm"
              className="bg-black/5 dark:bg-white/5 text-gray-600 dark:text-gray-300 hover:bg-black/10 dark:hover:bg-white/10"
              onPress={handleResetRunbookState}
            >
              <RefreshCcwIcon className="h-4 w-4" />
            </Button>
          </Tooltip>
          <Tooltip content="Runbook settings" placement="bottom">
            <Button
              isIconOnly
              variant="flat"
              size="sm"
              className={cn(
                "bg-black/5 dark:bg-white/5",
                props.isSettingsOpen
                  ? "bg-black/15 dark:bg-white/15 text-gray-800 dark:text-gray-100"
                  : "text-gray-600 dark:text-gray-300 hover:bg-black/10 dark:hover:bg-white/10"
              )}
              onPress={props.onToggleSettings}
            >
              <SettingsIcon className="h-4 w-4" />
            </Button>
          </Tooltip>
          <Tooltip
            content={
              props.isAIFeaturesEnabled
                ? "AI assistant"
                : "AI features disabled. Enable in Settings → AI."
            }
            placement="bottom"
          >
            <Button
              isIconOnly
              variant="flat"
              size="sm"
              isDisabled={!props.isAIFeaturesEnabled}
              className={cn(
                "bg-black/5 dark:bg-white/5",
                !props.isAIFeaturesEnabled
                  ? "opacity-50 text-gray-400 dark:text-gray-500"
                  : props.isAIAssistantOpen
                    ? "bg-purple-500/20 dark:bg-purple-400/20 text-purple-600 dark:text-purple-300"
                    : "text-purple-500 dark:text-purple-400 hover:bg-black/10 dark:hover:bg-white/10"
              )}
              onPress={props.isAIFeaturesEnabled ? props.toggleAIAssistant : undefined}
            >
              <SparklesIcon className="h-4 w-4" />
            </Button>
          </Tooltip>
          <PlayButton
            isRunning={serialExecution.isRunning}
            cancellable={true}
            tooltip="Run runbook"
            tooltipPlacement="bottom"
            onPlay={handleStartSerialExecution}
            onStop={handleStopSerialExecution}
          />
        </ButtonGroup>
      </div>
    );
  };

  return (
    <div className="flex w-full max-w-full overflow-hidden bg-gray-50 dark:bg-content1 h-[60px] min-h-[60px] flex-row items-center border-b dark:border-default-300 px-3">
      {workspace && renderBarContents()}
    </div>
  );
}
