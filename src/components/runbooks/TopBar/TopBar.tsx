import Runbook from "@/state/runbooks/runbook";
import RelativeTime from "@/components/relative_time.tsx";
import TagSelector from "./TagSelector";
import ColorAvatar from "@/components/ColorAvatar";
import { DateTime } from "luxon";
import { addToast, Avatar, AvatarGroup, Button, Tooltip } from "@heroui/react";
import { RemoteRunbook } from "@/state/models";
import { BookTextIcon, CopyIcon, PencilOffIcon, SettingsIcon, TrashIcon } from "lucide-react";
import { PresenceUserInfo } from "@/lib/phoenix_provider";
import { useQuery } from "@tanstack/react-query";
import { workspaceById } from "@/lib/queries/workspaces";
import { useStore } from "@/state/store";
import BlockBus from "@/lib/workflow/block_bus";
import { invoke } from "@tauri-apps/api/core";
import track_event from "@/tracking";
import PlayButton from "@/lib/blocks/common/PlayButton";
import AtuinEnv from "@/atuin_env";
import { open } from "@tauri-apps/plugin-shell";
import { cn } from "@/lib/utils";

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
};

function openHubRunbook(e: React.MouseEvent<HTMLAnchorElement>) {
  e.preventDefault();
  open(e.currentTarget.href);
}

export default function Topbar(props: TopbarProps) {
  let runbook = props.runbook;
  let remoteRunbook = props.remoteRunbook;
  let serialExecution = useStore((state) => state.serialExecution);
  let startSerialExecution = useStore((state) => state.startSerialExecution);
  let stopSerialExecution = useStore((state) => state.stopSerialExecution);
  let { data: workspace } = useQuery(workspaceById(runbook.workspaceId));

  let name: string;
  if (remoteRunbook) {
    name = remoteRunbook.nwo;
  } else {
    name = runbook.name;
  }

  function onSelectTag(tag: string) {
    props.onSelectTag(tag);
  }

  function handleDeleteTag() {
    if (!props.currentTag) {
      return;
    }

    props.onDeleteTag(props.currentTag);
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
                    onClick={() => {
                      navigator.clipboard.writeText(AtuinEnv.url(remoteRunbook.nwo));
                      addToast({
                        title: "Runbook URL copied to clipboard",
                        color: "success",
                        radius: "sm",
                        timeout: 2000,
                        shouldShowTimeoutProgress: false,
                      });
                    }}
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
        <Tooltip content="Toggle runbook settings" placement="left" showArrow>
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
        <PlayButton
          className="mt-1 ml-2"
          isRunning={serialExecution.includes(runbook.id)}
          disabled={serialExecution.length > 0 && !serialExecution.includes(runbook.id)}
          cancellable={true}
          onPlay={() => {
            track_event("runbooks.serial.execute");
            BlockBus.get().startWorkflow(runbook.id);
            startSerialExecution(runbook.id);

            const onStop = () => {
              stopSerialExecution(runbook.id);
              BlockBus.get().unsubscribeWorkflowFinished(runbook.id, onStop);
            };

            BlockBus.get().subscribeWorkflowFinished(runbook.id, onStop);
          }}
          onStop={async () => {
            await invoke("workflow_stop", { id: runbook.id });
          }}
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
