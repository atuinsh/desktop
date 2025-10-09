import { useEffect, useState } from "react";
import {
  Button,
  Input,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  Tooltip,
} from "@heroui/react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderIcon } from "lucide-react";
import { Option, Some } from "@binarymuse/ts-stdlib";
import { cn } from "@/lib/utils";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import { readDir } from "@tauri-apps/plugin-fs";
import * as commands from "@/lib/workspaces/commands";
import { findParentWorkspace } from "@/lib/workspaces/offline_strategy";

interface NewWorkspaceDialogProps {
  onAccept: (name: string, online: boolean, folder: Option<string>) => void;
  onCancel: () => void;
}

export default function NewWorkspaceDialog({ onAccept, onCancel }: NewWorkspaceDialogProps) {
  const [name, setName] = useState("New Workspace");
  const [isOnline, setIsOnline] = useState(true);
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null);
  const [folderHasContents, setFolderHasContents] = useState(false);
  const [existingWorkspaceId, setExistingWorkspaceId] = useState<string | null>(null);
  const [isChildOfWorkspace, setIsChildOfWorkspace] = useState(false);
  const connectionState = useStore((state) => state.connectionState);

  useEffect(() => {
    if (!selectedFolder) return;
    let active = true;
    setFolderHasContents(false);
    setExistingWorkspaceId(null);
    setIsChildOfWorkspace(false);

    readDir(selectedFolder).then(async (dir) => {
      if (dir.length > 0 && active) {
        setFolderHasContents(true);

        const result = await commands.getWorkspaceIdByFolder(selectedFolder);
        if (result.isOk() && active) {
          setExistingWorkspaceId(result.unwrap());
        }
      }

      const parentWorkspace = await findParentWorkspace(selectedFolder);
      if (parentWorkspace.isSome() && parentWorkspace.unwrap() !== selectedFolder && active) {
        setIsChildOfWorkspace(true);
      }
    });

    return () => {
      active = false;
    };
  }, [selectedFolder]);

  function closeAndReset() {
    onCancel();
    setName("New Workspace");
    setIsOnline(true);
    setSelectedFolder(null);
  }

  function handleSubmit() {
    onAccept(name, isOnline, Some(selectedFolder));
  }

  return (
    <Modal isOpen={true} onClose={closeAndReset}>
      <ModalContent>
        <ModalHeader>Create or Open a Workspace</ModalHeader>
        <ModalBody>
          <div className="space-y-4">
            <Input
              label="Workspace Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
            />

            <div>
              <label className="block text-sm font-medium mb-2">Workspace Type</label>
              <div className="space-y-2">
                <div>
                  <label className="flex items-center">
                    <input
                      type="radio"
                      checked={isOnline}
                      onChange={() => setIsOnline(true)}
                      className="mr-2"
                    />
                    <span>Online - Sync across devices</span>
                  </label>
                </div>
                {isOnline && (
                  <div className="ml-6 text-sm">
                    {connectionState === ConnectionState.Offline && (
                      <span className="text-red-500">
                        You are offline. You must be online to create an online workspace.
                      </span>
                    )}
                    {connectionState === ConnectionState.LoggedOut && (
                      <span className="text-red-500">
                        You must be logged in to Atuin Hub to create an online workspace.
                      </span>
                    )}
                    {connectionState === ConnectionState.OutOfDate && (
                      <span className="text-red-500">
                        You must update Atuin Desktop to the latest version to create an online
                        workspace.
                      </span>
                    )}
                  </div>
                )}
                <div>
                  <label className="flex items-center">
                    <input
                      type="radio"
                      checked={!isOnline}
                      onChange={() => setIsOnline(false)}
                      className="mr-2"
                    />
                    <div>Offline - Local only</div>
                  </label>
                </div>
                <div className="flex items-center">
                  <Tooltip content="Select a folder to store your workspace locally">
                    <Button
                      isIconOnly
                      isDisabled={isOnline}
                      className="mr-2"
                      onPress={() => {
                        open({
                          directory: true,
                        }).then((folder) => {
                          setSelectedFolder(folder);
                        });
                      }}
                    >
                      <FolderIcon />
                    </Button>
                  </Tooltip>
                  <span
                    className={cn(
                      !isOnline ? "text-foreground" : "text-muted",
                      "overflow-x-auto",
                      "whitespace-nowrap",
                      "flex-grow",
                    )}
                  >
                    {!selectedFolder && "No folder selected"}
                    {selectedFolder && selectedFolder}
                  </span>
                </div>
                {selectedFolder && isChildOfWorkspace && (
                  <div className="text-danger-500 mt-2">
                    The selected folder is a child of an existing workspace. Please choose a
                    different folder.
                  </div>
                )}
                {selectedFolder &&
                  folderHasContents &&
                  !existingWorkspaceId &&
                  !isChildOfWorkspace && (
                    <div className="text-danger-500 mt-2">
                      The selected folder is not empty. Any conflicting files will be overwritten.
                    </div>
                  )}
                {selectedFolder && folderHasContents && existingWorkspaceId && (
                  <div className="text-warning-500 mt-2">
                    The selected folder already contains a workspace. This workspace will be added
                    to Atuin Desktop.
                  </div>
                )}
              </div>
            </div>
          </div>
        </ModalBody>
        <ModalFooter>
          <Button onPress={closeAndReset} variant="flat">
            Cancel
          </Button>
          <Button
            onPress={handleSubmit}
            color="primary"
            isDisabled={
              !name ||
              (!isOnline && (!selectedFolder || isChildOfWorkspace)) ||
              (isOnline && connectionState !== ConnectionState.Online)
            }
          >
            Create Workspace
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
