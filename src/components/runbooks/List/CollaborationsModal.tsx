import * as api from "@/api/api";
import AtuinEnv from "@/atuin_env";
import { allWorkspaces } from "@/lib/queries/workspaces";
import RunbookSynchronizer from "@/lib/sync/runbook_synchronizer";
import Workspace from "@/state/runbooks/workspace";
import { useStore } from "@/state/store";
import { Collaboration } from "@/state/store/collaboration_state";
import { ConnectionState } from "@/state/store/user_state";
import {
  Button,
  ButtonGroup,
  Dropdown,
  DropdownItem,
  DropdownMenu,
  DropdownTrigger,
  Modal,
  ModalBody,
  ModalContent,
  ModalHeader,
} from "@heroui/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-shell";
import { ChevronDownIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

interface CollaborationsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type Acceptance = {
  collaboration: Collaboration;
  workspaceId: string;
};

export default function CollaborationsModal(props: CollaborationsModalProps) {
  const connectionState = useStore((s) => s.connectionState);
  const currentWorkspaceId = useStore((s) => s.currentWorkspaceId);
  const user = useStore((s) => s.user);
  const collaborations = useStore((s) => s.collaborations);
  const refreshCollaborations = useStore((s) => s.refreshCollaborations);
  const markCollaborationAccepted = useStore((s) => s.markCollaborationAccepted);
  const removeCollaboration = useStore((s) => s.removeCollaboration);
  const pendingCollabs = useMemo(() => {
    return collaborations.filter((c) => !c.accepted);
  }, [collaborations]);
  const wsQuery = useQuery(allWorkspaces());
  const workspaces = wsQuery.data || [];
  const currentWorkspace = useMemo(() => {
    return workspaces.find((ws) => ws.get("id") === currentWorkspaceId);
  }, [workspaces, currentWorkspaceId]);
  const [acceptIntoWs, setAcceptIntoWs] = useState<Record<string, Workspace>>({});

  // const { runbookCreated } = useContext(RunbookContext);

  function setWorkspaceForCollab(collabId: string, workspace: Workspace) {
    setAcceptIntoWs((map) => {
      map[collabId] = workspace;
      return { ...map };
    });
  }

  const acceptCollabMutation = useMutation({
    mutationFn: async (acceptance: Acceptance) => {
      return api.acceptCollaboration(acceptance.collaboration.id);
    },
    onSuccess: (_data, acceptance) => {
      const { collaboration, workspaceId } = acceptance;
      new RunbookSynchronizer(collaboration.runbook.id, workspaceId, user)
        .sync()
        .then(async (result) => {
          if (result.action === "created") {
            // TODO
            // runbookCreated(collaboration.runbook.id, workspaceId, null, false);
          }
        });
    },
    onSettled: () => refreshCollaborations(),
    scope: { id: "collaborations" },
  });

  const declineCollabMutation = useMutation({
    mutationFn: async (collab: Collaboration) => {
      return api.declineCollaboration(collab.id);
    },
    onSuccess: (_data, collab) => {
      new RunbookSynchronizer(collab.runbook.id, currentWorkspaceId, user).sync();
    },
    onSettled: () => refreshCollaborations(),
    scope: { id: "collaborations" },
  });

  useEffect(() => {
    if (props.isOpen) refreshCollaborations();
  }, [props.isOpen]);

  function acceptInvitation(collab: Collaboration) {
    markCollaborationAccepted(collab.id);
    acceptCollabMutation.mutate({
      collaboration: collab,
      workspaceId: acceptIntoWs[collab.id]?.get("id") || currentWorkspaceId,
    });
  }

  function declineInvitation(collab: Collaboration) {
    removeCollaboration(collab.id);
    declineCollabMutation.mutate(collab);
  }

  function collabRunbookUrl(collab: Collaboration) {
    return AtuinEnv.url(`/${collab.runbook.owner}/${collab.runbook.slug}`);
  }

  function openRunbookLink(evt: React.MouseEvent<HTMLAnchorElement, MouseEvent>) {
    evt.preventDefault();
    const target = evt.target as HTMLAnchorElement;
    const href = target.getAttribute("href");
    open(href!);
  }

  return (
    <Modal isOpen={props.isOpen} size="2xl" scrollBehavior="inside" onClose={props.onClose}>
      <ModalContent className="pb-4 max-h-[60vh]">
        {(_onClose) => (
          <>
            <ModalHeader className="flex flex-col gap-1">
              <h1 className="text-2xl font-bold">Collaboration Invitations</h1>
              <p className="text-small text-default-500">
                Accept or decline invitations to collaborate with other users
              </p>
            </ModalHeader>
            <ModalBody className="block">
              {connectionState !== ConnectionState.Online && (
                <p className="">
                  You must be online and logged in to accept or decline collaboration invitations.
                </p>
              )}
              {connectionState === ConnectionState.Online && pendingCollabs.length === 0 && (
                <p>No pending invitations</p>
              )}
              {connectionState === ConnectionState.Online && pendingCollabs.length > 0 && (
                <ul className="list-disc ml-6">
                  {pendingCollabs.map((collab) => {
                    const workspace = acceptIntoWs[collab.id] || currentWorkspace;
                    return (
                      <li key={collab.id} className="mb-4">
                        <div>
                          <a
                            href={collabRunbookUrl(collab)}
                            className="underline"
                            onClick={openRunbookLink}
                          >
                            {collab.runbook.owner} / {collab.runbook.name}
                          </a>
                        </div>
                        <div className="flex gap-2 mt-2">
                          <ButtonGroup variant="flat">
                            <Button
                              size="md"
                              color="success"
                              variant="flat"
                              onClick={() => acceptInvitation(collab)}
                              isDisabled={connectionState !== ConnectionState.Online}
                            >
                              Accept into {workspace.get("name")}
                            </Button>
                            <Dropdown placement="bottom-end">
                              <DropdownTrigger>
                                <Button
                                  isIconOnly
                                  color="success"
                                  className="border-l-1 border-gray-300"
                                >
                                  <ChevronDownIcon />
                                </Button>
                              </DropdownTrigger>
                              <DropdownMenu
                                items={workspaces.filter((ws) => {
                                  ws.get("id") !== workspace.get("id") && ws.canManageRunbooks();
                                })}
                              >
                                {(ws) => (
                                  <DropdownItem
                                    key={ws.get("id")!}
                                    onClick={() => setWorkspaceForCollab(collab.id, ws)}
                                  >
                                    Accept into {ws.get("name")}
                                  </DropdownItem>
                                )}
                              </DropdownMenu>
                            </Dropdown>
                          </ButtonGroup>
                          <Button
                            size="md"
                            color="danger"
                            variant="flat"
                            onClick={() => declineInvitation(collab)}
                            isDisabled={connectionState !== ConnectionState.Online}
                          >
                            Decline
                          </Button>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}
            </ModalBody>
          </>
        )}
      </ModalContent>
    </Modal>
  );
}
