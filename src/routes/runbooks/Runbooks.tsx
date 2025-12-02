import * as Sentry from "@sentry/react";
import useRemoteRunbook from "@/lib/useRemoteRunbook";
import { usePtyStore } from "@/state/ptyStore";
import { useStore } from "@/state/store";
import Snapshot from "@/state/runbooks/snapshot";
import React, { useContext, useEffect, useMemo, useRef, useState } from "react";
import { timeoutPromise, useMemory } from "@/lib/utils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as api from "@/api/api";
import { snapshotByRunbookAndTag, snapshotsByRunbook } from "@/lib/queries/snapshots";
import Runbook from "@/state/runbooks/runbook";
import { PresenceUserInfo } from "@/lib/phoenix_provider";
import RunbookEditor from "@/lib/runbook_editor";
import Operation from "@/state/runbooks/operation";
import { ConnectionState } from "@/state/store/user_state";
import { DialogBuilder } from "@/components/Dialogs/dialog";
import AppBus from "@/lib/app/app_bus";
import { workspaceById } from "@/lib/queries/workspaces";
import WorkspaceManager from "@/lib/workspaces/manager";
import { runbookById } from "@/lib/queries/runbooks";
import { useParams } from "react-router-dom";
import { TabsContext } from "../root/Tabs";
import RunbookIdContext from "@/context/runbook_id_context";
import { invoke } from "@tauri-apps/api/core";
import RunbookSynchronizer from "@/lib/sync/runbook_synchronizer";
import RunbookControls from "./RunbookControls";
import {
  DocumentBridge,
  DocumentBridgeContext,
  useBlockContext,
} from "@/lib/hooks/useDocumentBridge";
import DebugWindow from "@/lib/dev/DebugWindow";
import { useSerialExecution } from "@/lib/hooks/useSerialExecution";
import { Button, Spinner } from "@heroui/react";

const Editor = React.lazy(() => import("@/components/runbooks/editor/Editor"));
const Topbar = React.lazy(() => import("@/components/runbooks/TopBar/TopBar"));

function useMarkRunbookRead(runbook: Runbook | null, refreshRunbooks: () => void) {
  useEffect(() => {
    if (runbook) {
      runbook.markViewed().then(() => {
        refreshRunbooks();
      });
    }
  }, [runbook?.id]);
}

export default function Runbooks() {
  const { runbookId } = useParams();

  const user = useStore((store) => store.user);
  const connectionState = useStore((store) => store.connectionState);
  const refreshRunbooks = useStore((store) => store.refreshRunbooks);
  const getLastTagForRunbook = useStore((store) => store.getLastTagForRunbook);
  const setLastTagForRunbook = useStore((store) => store.selectTag);
  const {
    data: currentRunbook,
    isLoading: currentRunbookLoading,
    isFetching: isFetchingRunbook,
    isError: isErrorFetchingRunbook,
  } = useQuery(runbookById(runbookId));
  const { data: runbookWorkspace } = useQuery(workspaceById(currentRunbook?.workspaceId || null));
  const lastRunbookRef = useMemory(currentRunbook);
  const [presences, setPresences] = useState<PresenceUserInfo[]>([]);
  const [runbookEditor, setRunbookEditor] = useState<RunbookEditor | null>(null);
  const lastRunbookEditor = useRef<RunbookEditor | null>(runbookEditor);
  const serialExecution = useSerialExecution(runbookId);
  const { tab, ...tabsApi } = useContext(TabsContext);
  const registerTabOnClose = useStore((store) => store.registerTabOnClose);
  const setCurrentWorkspaceId = useStore((store) => store.setCurrentWorkspaceId);
  const ptys = usePtyStore((state) => state.ptys);
  const activePtyCount = Object.values(ptys).filter((pty) => pty.runbook === runbookId).length;

  const [documentOpened, setDocumentOpened] = useState(false);
  const [syncingRunbook, setSyncingRunbook] = useState(false);
  const [failedToSyncRunbook, setFailedToSyncRunbook] = useState(false);
  // Key used to re-render editor when making major changes to runbook
  const [editorKey, setEditorKey] = useState<boolean>(false);
  const [showTagMenu, setShowTagMenu] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [selectedTag, setSelectedTag] = useState<string | null>(() => {
    let tag = currentRunbook ? getLastTagForRunbook(currentRunbook.id) : null;
    if (tag == "(no tag)") tag = null;

    return tag;
  });

  useEffect(
    function syncRunbookIfNotSynced() {
      if (!runbookId) return;
      if (currentRunbookLoading) return;
      if (currentRunbook) return;

      // If this runbook is in an online workspace and doesn't exist in the local database
      // (due to background sync being off), we need to sync it. However, we don't know yet
      // if the runbook is in an online workspace or not, as the offline workspace
      // manager may take some time to return the runbook - but we don't want to wait until
      // the offline workspace manager times out before we decide this must be an online runbook.
      // So, we'll attempt to sync as soon as `isPending` is false for the runbook query.

      (async function syncRunbook() {
        setSyncingRunbook(true);
        // If the runbook wasn't synced when this tab was loaded,
        // we don't know the workspace ID.
        const sync = new RunbookSynchronizer(runbookId, null, user);
        try {
          await sync.sync(false); // yjs sync will happen on open
          const runbook = await Runbook.load(runbookId);
          if (runbook) {
            // If the runbook wasn't synced when this tab was loaded,
            // we need to set the workspace ID.
            setCurrentWorkspaceId(runbook.workspaceId);
          } else {
            throw new Error(`Runbook ${runbookId} not found after sync`);
          }
        } catch (err) {
          setFailedToSyncRunbook(true);
          console.warn(
            "Error syncing runbook; this could be normal if the runbook is offline",
            err,
          );
        } finally {
          setSyncingRunbook(false);
        }
      })();
    },
    [currentRunbookLoading, currentRunbook, runbookId, user],
  );

  const [documentBridge, setDocumentBridge] = useState<DocumentBridge | null>(null);

  useEffect(() => {
    if (!currentRunbook?.id) {
      return;
    }
    setDocumentBridge(new DocumentBridge(currentRunbook.id));
  }, [currentRunbook?.id]);

  const [focusedBlockId, setFocusedBlockId] = useState<string | null>(null);
  const onBlockFocus = (blockId: string) => {
    console.log("block focus", blockId);
    setFocusedBlockId(blockId);
  };

  useEffect(() => {
    if (!runbookEditor) {
      return;
    }

    return runbookEditor.onBlockFocus(onBlockFocus);
  }, [runbookEditor]);

  useEffect(() => {
    if (!currentRunbook) {
      return;
    }

    tabsApi.setPtyCount(activePtyCount);
  }, [activePtyCount, currentRunbook]);

  useEffect(() => {
    if (!tab || !currentRunbook) {
      return;
    }

    return registerTabOnClose(tab.id, async () => {
      if (serialExecution.isRunning) {
        const answer = await new DialogBuilder()
          .title(`Cancel workflow execution?`)
          .icon("question")
          .message(
            `You are currently executing a workflow in the Runbook "${currentRunbook.name}". Closing this tab will stop the workflow.`,
          )
          .action({
            label: "Cancel",
            value: "cancel",
          })
          .action({
            label: "Stop and Close",
            value: "ok",
            color: "danger",
          })
          .build();

        if (answer === "ok") {
          try {
            serialExecution.stop();
          } catch (error) {
            console.error("Error stopping workflow", error);
            return false;
          }
          await timeoutPromise(250, undefined);
          return true;
        } else {
          return false;
        }
      }

      return true;
    });
  }, [currentRunbook?.id, tab?.id, serialExecution.isRunning]);

  useEffect(() => {
    if (currentRunbook) {
      tabsApi.setTitle(currentRunbook.name);
    }
  }, [currentRunbook?.name]);

  useEffect(() => {
    if (currentRunbook && documentBridge) {
      invoke("open_document", {
        documentId: currentRunbook.id,
        document: currentRunbook.content ? JSON.parse(currentRunbook.content) : "[]",
        documentBridge: documentBridge.channel,
      })
        .then(() => {
          setDocumentOpened(true);
        })
        .catch((err) => {
          console.error("Error opening document in runtime backend", err);
        });
    }
  }, [currentRunbook?.id, documentBridge?.channel]);

  useMarkRunbookRead(currentRunbook || null, refreshRunbooks);

  const queryClient = useQueryClient();

  const [remoteRunbook, refreshRemoteRunbook] = useRemoteRunbook(currentRunbook || undefined);
  const { data: currentSnapshot } = useQuery(
    snapshotByRunbookAndTag(currentRunbook?.id, selectedTag),
  );
  const { data: snapshots, isFetching: snapshotsFetching } = useQuery(
    snapshotsByRunbook(currentRunbook?.id),
  );

  useEffect(() => {
    if (remoteRunbook && !remoteRunbook.owner) {
      console.log("Refreshing remote runbook");
      refreshRemoteRunbook();
    }
  }, [remoteRunbook]);

  const tags = useMemo(() => {
    let tags = (snapshots || []).map((snap) => ({ text: snap.tag, value: snap.tag })) || [];
    if (!remoteRunbook || remoteRunbook?.permissions.includes("update_content")) {
      tags = [{ text: "(no tag)", value: "latest" }, ...tags];
    }

    return tags;
  }, [snapshots, remoteRunbook]);

  function updateEditorKey() {
    setEditorKey((prev) => !prev);
  }

  const shareSnapshot = useMutation({
    mutationFn: async (snapshot: Snapshot) => {
      return api.createSnapshot(snapshot);
    },
    onSuccess: (_data, snapshot) => {
      console.info(`Successfully created snapshot ${snapshot.tag}`);
    },
    onError: (err: any) => {
      console.error("Error creating snapshot", err);
    },
    scope: { id: `runbook` },
  });

  useEffect(() => {
    setPresences([]);
  }, [currentRunbook?.id, currentSnapshot?.id]);

  function onPresenceJoin(user: PresenceUserInfo) {
    setPresences((prev) => {
      const index = prev.findIndex((u) => u.id == user.id);
      if (index > -1) {
        const before = prev.slice(0, index);
        const after = prev.slice(index + 1);
        return [...before, user, ...after];
      } else {
        return [...prev, user];
      }
    });
  }

  function onPresenceLeave(user: PresenceUserInfo) {
    setPresences((prev) => prev.filter((u) => u.id != user.id));
  }

  function onClearPresences() {
    setPresences([]);
  }

  useEffect(() => {
    if (!currentRunbook) {
      setSelectedTag(null);
      return;
    }

    refreshRemoteRunbook();

    let tag = getLastTagForRunbook(currentRunbook.id);
    if (tag == "(no tag)") tag = null;
    setSelectedTag(tag);
  }, [currentRunbook?.id]);

  useEffect(() => {
    if (!snapshots || !currentRunbook || snapshotsFetching) return;

    const tagExists = tags.some((tag) => tag.value == selectedTag);
    if (tagExists) return;

    if (!tagExists && tags.some((tag) => tag.value == "latest")) {
      setSelectedTag("latest");
    } else if (!tagExists) {
      setSelectedTag(tags[0]?.value || null);
    }
  }, [selectedTag, snapshots, tags, snapshotsFetching]);

  useEffect(() => {
    const workspaceManager = WorkspaceManager.getInstance();
    const unsub = workspaceManager.onRunbookChanged(async (runbook, contentHash) => {
      if (runbook.id === currentRunbook?.id) {
        runbookEditor?.runbookUpdatedExternally(runbook, contentHash);
      }
    });

    return unsub;
  }, [currentRunbook?.id, runbookEditor]);

  function handleSelectTag(tag: string | null) {
    if (!tag) tag = "latest";
    setSelectedTag(tag);
    setShowTagMenu(false);
    if (currentRunbook) {
      setLastTagForRunbook(currentRunbook.id, tag);
    }
  }

  async function handleCreateTag(tag: string) {
    if (!currentRunbook) {
      throw new Error("Tried to create a new tag with no runbook selected");
    }

    let snapshot = await Snapshot.create({
      id: undefined,
      tag,
      runbook_id: currentRunbook.id,
      content: currentRunbook.content,
    });
    queryClient.invalidateQueries({ queryKey: snapshotsByRunbook(currentRunbook.id).queryKey });

    if (remoteRunbook) {
      shareSnapshot.mutate(snapshot, {
        onError: (_err) => {
          // TODO: how to handle if creating remote snapshot fails?
        },
      });
    }

    setLastTagForRunbook(currentRunbook.id, snapshot.tag);
    if (currentRunbook == lastRunbookRef.current) {
      setSelectedTag(snapshot.tag);
      setShowTagMenu(false);
    }
  }

  async function handleDeleteTag(tag: string) {
    if (!currentRunbook) {
      throw new Error("Tried to delete a tag with no runbook selected");
    }

    let snaps = snapshots || [];

    const currentIndex = snaps.findIndex((snap) => snap.tag == tag);
    if (currentIndex == -1) return;

    const current = snaps[currentIndex];
    const before = snaps[currentIndex - 1];
    const after = snaps[currentIndex + 1];
    let newTag = after ? after.tag : before ? before.tag : "latest";

    setSelectedTag(newTag);
    const id = current.id;
    await current.delete();

    if (connectionState !== ConnectionState.Offline) {
      const op = new Operation({
        operation: { type: "snapshot_deleted", snapshotId: id },
      });
      await op.save();
    } else {
      await api.deleteSnapshot(current.id);
    }
  }

  async function handleSharedToHub() {
    await currentRunbook?.clearRemoteInfo();
    lastRunbookEditor.current?.resetEditor();
    refreshRemoteRunbook();
    updateEditorKey();
  }

  async function handleDeletedFromHub() {
    await currentRunbook?.clearRemoteInfo();
    lastRunbookEditor.current?.resetEditor();
    refreshRemoteRunbook();
    updateEditorKey();
  }

  useEffect(() => {
    const unsub = AppBus.get().onResetEditor((rbId: string) => {
      console.log("Resetting editor instance", rbId, currentRunbook?.id);
      if (rbId == currentRunbook?.id) {
        lastRunbookEditor.current?.resetEditor();
        updateEditorKey();
      }
    });

    return unsub;
  }, [currentRunbook?.id]);

  function handleShowTagMenu() {
    if (currentRunbook && serialExecution.isRunning) {
      new DialogBuilder()
        .title("Cannot switch tags")
        .message("You cannot switch tags while a runbook is executing a workflow.")
        .action({
          label: "OK",
          value: "ok",
          color: "primary",
        })
        .build();
    } else {
      setShowTagMenu(true);
    }
  }

  useEffect(() => {
    if (!currentRunbook || !runbookWorkspace) {
      return;
    }

    const newRunbookEditor = new RunbookEditor(
      currentRunbook,
      user,
      selectedTag,
      runbookWorkspace.isOnline(),
      onPresenceJoin,
      onPresenceLeave,
      onClearPresences,
    );
    lastRunbookEditor.current = newRunbookEditor;
    setEditorKey((prev) => !prev);
    setRunbookEditor(newRunbookEditor);

    return () => {
      if (lastRunbookEditor.current) {
        lastRunbookEditor.current.shutdown();
        setRunbookEditor(null);
      }
    };
  }, [currentRunbook?.id, runbookWorkspace?.get("id")]);

  useEffect(() => {
    if (!currentRunbook || !runbookEditor) return;

    if (runbookEditor.runbook.id !== currentRunbook.id) return;

    runbookEditor.updateRunbook(currentRunbook);
    runbookEditor.updateUser(user);
    runbookEditor.updateSelectedTag(selectedTag);
    runbookEditor.setOnline(runbookWorkspace?.isOnline() || false);
    setRunbookEditor(runbookEditor);
  }, [runbookEditor, currentRunbook, user, selectedTag, runbookWorkspace?.isOnline()]);

  const editable = !remoteRunbook || remoteRunbook?.permissions.includes("update_content");
  const canEditTags = !remoteRunbook || remoteRunbook?.permissions.includes("update");
  const canInviteCollabs = remoteRunbook?.permissions.includes("update");
  const hasNoTags = tags.length == 0;

  const readyToRender =
    documentBridge &&
    documentOpened &&
    runbookEditor &&
    (selectedTag == "latest" ||
      (currentSnapshot && selectedTag == currentSnapshot.tag) ||
      (selectedTag == null && hasNoTags));

  return (
    <RunbookIdContext.Provider value={currentRunbook?.id || null}>
      <DocumentBridgeContext.Provider value={documentBridge}>
        <div className="flex !w-full !max-w-full flex-row overflow-hidden h-full">
          {runbookId && focusedBlockId && (
            <BlockContextDebug runbookId={runbookId} blockId={focusedBlockId} />
          )}
          {currentRunbook && readyToRender && (
            <div className="flex w-full max-w-full overflow-hidden flex-col">
              <Topbar
                runbook={currentRunbook}
                remoteRunbook={remoteRunbook || undefined}
                tags={tags}
                presences={presences}
                showTagMenu={showTagMenu}
                onOpenTagMenu={handleShowTagMenu}
                onCloseTagMenu={() => setShowTagMenu(false)}
                currentTag={selectedTag}
                onSelectTag={handleSelectTag}
                canEditTags={canEditTags}
                canInviteCollaborators={!!canInviteCollabs}
                onCreateTag={handleCreateTag}
                onDeleteTag={handleDeleteTag}
                onShareToHub={handleSharedToHub}
                onDeleteFromHub={handleDeletedFromHub}
                onToggleSettings={() => setShowSettings((show) => !show)}
                isSettingsOpen={showSettings}
              />
              {showSettings && runbookWorkspace && (
                <RunbookControls
                  runbook={currentRunbook}
                  remoteRunbook={remoteRunbook || undefined}
                  isOrgOwned={runbookWorkspace.isOrgOwned()}
                  isOfflineRunbook={
                    !runbookWorkspace.isOnline() && !runbookWorkspace.isLegacyHybrid()
                  }
                  onClose={() => setShowSettings(false)}
                />
              )}
              <Sentry.ErrorBoundary showDialog={false}>
                {!hasNoTags && (
                  <Editor
                    key={editorKey ? "1" : "2"}
                    runbook={currentRunbook}
                    runbookEditor={runbookEditor}
                    editable={editable && selectedTag == "latest"}
                  />
                )}
                {hasNoTags && (
                  <div className="flex align-middle justify-center flex-col h-screen w-full">
                    <h1 className="text-center">This runbook has no published tags</h1>
                  </div>
                )}
              </Sentry.ErrorBoundary>
            </div>
          )}

          {!currentRunbook && failedToSyncRunbook && isErrorFetchingRunbook && (
            <div className="flex align-middle justify-center flex-col h-screen w-full">
              <h1 className="text-center">We were unable to load this runbook.</h1>
              <Button
                className="inline-block mx-auto"
                onPress={() => {
                  tabsApi.reloadTab();
                }}
              >
                Retry
              </Button>
            </div>
          )}

          {!currentRunbook && (syncingRunbook || isFetchingRunbook) && (
            <div className="flex align-middle justify-center flex-col h-screen w-full">
              <h1 className="text-center">Loading runbook, please wait...</h1>
              <Spinner />
            </div>
          )}
        </div>
      </DocumentBridgeContext.Provider>
    </RunbookIdContext.Provider>
  );
}

function BlockContextDebug({ runbookId, blockId }: { runbookId: string; blockId: string }) {
  const blockContext = useBlockContext(blockId);

  return (
    <DebugWindow title="Block Context" id={`block-context-${runbookId}`}>
      <pre>{JSON.stringify(blockContext, null, 2)}</pre>
    </DebugWindow>
  );
}
