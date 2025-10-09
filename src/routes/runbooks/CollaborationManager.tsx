import { RemoteRunbook, RemoteUser } from "@/state/models";
import Runbook from "@/state/runbooks/runbook";
import { Avatar, Button, Autocomplete, AutocompleteItem } from "@heroui/react";
import { useAsyncList } from "@react-stately/data";
import { useQueryClient, useMutation } from "@tanstack/react-query";
import { TrashIcon } from "lucide-react";
import { useState, useMemo } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import * as api from "@/api/api";

interface CollaborationManagerProps {
  runbook: Runbook;
  remoteRunbook: RemoteRunbook;
  disabled: boolean;
}

interface CreateCollabMutationArgs {
  runbookId: string;
  userId: string;
}

interface DeleteCollabMutationArgs {
  runbookId: string;
  collaborationId: string;
}

export default function CollaborationManager(props: CollaborationManagerProps) {
  const [selectedUser, setSelectedUser] = useState<string | null>(null);

  const existingCollabUsers = useMemo(() => {
    return new Set(props.remoteRunbook.collaborations.map((c) => c.user.id));
  }, [props.remoteRunbook]);

  const list = useAsyncList<RemoteUser>({
    async load({ filterText }) {
      if (!filterText || filterText.length <= 2) return { items: [] };

      let users = await api.searchUsers(filterText || "");

      return {
        items: users.filter((user) => !existingCollabUsers.has(user.id)),
      };
    },
  });

  const queryClient = useQueryClient();

  const inviteUserToCollabMutation = useMutation({
    mutationFn: ({ runbookId, userId }: CreateCollabMutationArgs) => {
      return api.createCollaborationInvitation(runbookId, userId);
    },
    onSuccess: (_data, vars) => {
      setSelectedUser(null);
      list.setFilterText("");
      queryClient.invalidateQueries({ queryKey: ["remote_runbook", vars.runbookId] });
    },
    onError: (_error: any) => {
      alert("Error inviting user to collaboration");
    },
  });

  const deleteCollaborationMutation = useMutation({
    mutationFn: ({ collaborationId }: DeleteCollabMutationArgs) => {
      return api.deleteCollaboration(collaborationId);
    },
    onSuccess: (_data, vars) => {
      queryClient.invalidateQueries({ queryKey: ["remote_runbook", vars.runbookId] });
    },
    onError: (_error: any) => {
      alert("Error deleting collaboration");
    },
  });

  function handleUserSelect(key: any) {
    setSelectedUser(key);
  }

  function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!selectedUser) return;
    inviteUserToCollabMutation.mutate({ runbookId: props.remoteRunbook.id, userId: selectedUser });
  }

  async function handleDeleteClicked(collaborationId: string) {
    const doDelete = await confirm(
      "Are you sure you want to remove this collaborator? This action cannot be undone.",
      { title: "Atuin Desktop", kind: "warning" },
    );

    if (!doDelete) return;

    deleteCollaborationMutation.mutate({
      runbookId: props.remoteRunbook.id,
      collaborationId,
    });
  }

  const mutationInProgress = inviteUserToCollabMutation.isPending;

  return (
    <>
      <form className="flex flex-row gap-2" onSubmit={handleSubmit}>
        <Autocomplete
          label="Username"
          inputValue={list.filterText}
          isLoading={list.isLoading}
          items={list.items}
          placeholder="Search for a user"
          variant="bordered"
          onInputChange={list.setFilterText}
          onSelectionChange={handleUserSelect}
          isDisabled={mutationInProgress || props.disabled}
        >
          {(user: RemoteUser) => (
            <AutocompleteItem
              key={user.id}
              startContent={
                <Avatar
                  alt={user.display_name || user.username}
                  src={user.avatar_url}
                  className="w-6 h-6"
                />
              }
            >
              {user.username}
            </AutocompleteItem>
          )}
        </Autocomplete>
        <Button
          type="submit"
          variant="flat"
          color="success"
          size="sm"
          className="py-4 px-8 h-full mt-1"
          isDisabled={!selectedUser || mutationInProgress || props.disabled}
        >
          Invite Collaborator
        </Button>
      </form>
      <ul className="max-h-[200px] overflow-y-auto mt-2">
        {props.remoteRunbook.collaborations.map((collaboration) => (
          <li key={collaboration.id} className="flex flex-row gap-2 justify-between mb-2">
            <div className="flex flex-row gap-2 items-center">
              <Avatar
                alt={collaboration.user.display_name || collaboration.user.username}
                src={collaboration.user.avatar_url}
                className="w-6 h-6"
              />
              <span>{collaboration.user.username}</span>
            </div>
            <div className="flex flex-row text-sm text-gray-500 items-center">
              <span>{collaboration.accepted ? "Accepted" : "Pending"}</span>
              <Button
                size="sm"
                variant="flat"
                color="danger"
                className="ml-2"
                isIconOnly
                onPress={() => handleDeleteClicked(collaboration.id)}
                isDisabled={props.disabled}
              >
                <TrashIcon size={16} />
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </>
  );
}
