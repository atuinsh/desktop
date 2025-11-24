import { remoteRunbook } from "@/lib/queries/runbooks";
import { allWorkspaces } from "@/lib/queries/workspaces";
import { getWorkspaceStrategy } from "@/lib/workspaces/strategy";
import Runbook, { OfflineRunbook, OnlineRunbook } from "@/state/runbooks/runbook";
import Workspace from "@/state/runbooks/workspace";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import {
  Button,
  cn,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  Select,
  SelectItem,
  Spinner,
} from "@heroui/react";
import { useQuery } from "@tanstack/react-query";
import { DynamicIcon } from "lucide-react/dynamic";
import { useEffect, useLayoutEffect, useMemo, useReducer, useState } from "react";

interface DesktopImportModalProps {
  runbookId: string;
  tag: string;
  activateRunbook: (runbookId: string) => Promise<void>;
  onClose: () => void;
}

type ModalStep = "checking" | "forked-options" | "import";

export default function DesktopImportModal(props: DesktopImportModalProps) {
  const connectionState = useStore((state) => state.connectionState);
  const currentWorkspaceId = useStore((state) => state.currentWorkspaceId);
  const workspaces = useQuery(allWorkspaces());
  const remoteRunbookQuery = useQuery(remoteRunbook(props.runbookId));
  const cannotImport =
    connectionState !== ConnectionState.Online && connectionState !== ConnectionState.LoggedOut;

  const [step, setStep] = useState<ModalStep>("checking");
  const [forkedRunbooks, setForkedRunbooks] = useState<Runbook[]>([]);
  const [selectedForkedRunbookId, setSelectedForkedRunbookId] = useState<string | null>(null);

  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(currentWorkspaceId);
  const selectedWorkspace = useMemo(() => {
    return workspaces.data?.find((ws) => ws.get("id") === selectedWorkspaceId) ?? null;
  }, [workspaces.data, selectedWorkspaceId]);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);

  // Check for existing forked runbooks when component mounts
  useEffect(() => {
    async function checkForForkedRunbooks() {
      try {
        const [onlineForked, offlineForked] = await Promise.all([
          OnlineRunbook.allForkedFrom(props.runbookId),
          OfflineRunbook.allForkedFrom(props.runbookId),
        ]);

        const allForked = [...onlineForked, ...offlineForked];

        if (allForked.length > 0) {
          setForkedRunbooks(allForked);
          setStep("forked-options");
        } else {
          setStep("import");
        }
      } catch (error) {
        console.error("Error checking for forked runbooks:", error);
        setStep("import");
      }
    }

    checkForForkedRunbooks();
  }, [props.runbookId]);

  function handleClose() {
    props.onClose();
  }

  async function handleOpenForkedRunbook(runbookId: string) {
    await props.activateRunbook(runbookId);
    props.onClose();
  }

  function handleImportNew() {
    setStep("import");
  }

  async function confirmImportRunbook() {
    if (!selectedWorkspaceId) return;

    setImportError(null);
    setImporting(true);

    try {
      const workspace = await Workspace.get(selectedWorkspaceId);
      if (!workspace) {
        throw new Error("Not able to load target workspace");
      }

      if (workspace.isOnline() && !workspace.canManageRunbooks()) {
        throw new Error("You do not have permission to manage runbooks in the target workspace");
      }

      const strategy = getWorkspaceStrategy(workspace);
      const result = await strategy.importRunbookFromHub(
        props.runbookId,
        props.tag,
        props.activateRunbook,
      );
      if (result.isErr()) {
        const err = result.unwrapErr();
        if ("message" in err.data) {
          throw new Error(err.data.message);
        }
        throw new Error("Failed to create runbook in the target workspace");
      }

      props.onClose();
    } catch (error) {
      setImportError(
        error instanceof Error
          ? error.message
          : typeof error === "string"
          ? error
          : "An unknown error occurred",
      );
    } finally {
      setImporting(false);
    }
  }

  const failed = remoteRunbookQuery.isError || workspaces.isError;
  const ready = remoteRunbookQuery.isSuccess && workspaces.isSuccess;

  let body: React.ReactNode | null = null;
  let footer: React.ReactNode | null = null;

  if (step === "checking") {
    body = (
      <div className="flex flex-col items-center justify-center gap-2">
        <Spinner />
        <p>Checking for existing copies...</p>
      </div>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Cancel</Button>
      </ModalFooter>
    );
  } else if (step === "forked-options") {
    const isImportNew = selectedForkedRunbookId === "__import_new__";
    const hasSelection = selectedForkedRunbookId !== null;

    body = (
      <div className="flex flex-col gap-4">
        <p>
          You already have {forkedRunbooks.length === 1 ? "a copy" : "copies"} of this runbook.
          Would you like to open an existing copy or import a new one?
        </p>
        <Select
          label="Choose an action"
          placeholder="Select an option"
          selectedKeys={selectedForkedRunbookId ? [selectedForkedRunbookId] : []}
          onSelectionChange={(keys) => {
            const selected = Array.from(keys)[0] as string | undefined;
            setSelectedForkedRunbookId(selected ?? null);
          }}
          items={[
            { id: "__import_new__", name: "Import as new copy" },
            ...forkedRunbooks.map((runbook) => ({
              id: runbook.id,
              name: `Open "${runbook.name || "Untitled"}"`,
            })),
          ]}
        >
          {(item) => <SelectItem key={item.id}>{item.name}</SelectItem>}
        </Select>
      </div>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Cancel</Button>
        <Button
          color="primary"
          isDisabled={!hasSelection}
          onPress={() => {
            if (isImportNew) {
              handleImportNew();
            } else if (selectedForkedRunbookId) {
              handleOpenForkedRunbook(selectedForkedRunbookId);
            }
          }}
        >
          {isImportNew ? "Import" : hasSelection ? "Open" : "Continue"}
        </Button>
      </ModalFooter>
    );
  } else if (importing) {
    body = (
      <div className="flex flex-col items-center justify-center gap-2">
        <p>Importing runbook...</p>
      </div>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose} isDisabled>
          Cancel
        </Button>
        <Button color="primary" isLoading>
          Import
        </Button>
      </ModalFooter>
    );
  } else if (importError) {
    body = (
      <p>
        Failed to import the runbook: <strong>{importError}</strong>
      </p>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Close</Button>
        <Button onPress={confirmImportRunbook} color="primary">
          Retry
        </Button>
      </ModalFooter>
    );
  } else if (failed) {
    body = (
      <p>
        Failed to load runbook information. The runbook may not exist or you may not have permission
        to access it.
      </p>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Close</Button>
      </ModalFooter>
    );
  } else if (cannotImport) {
    body = (
      <p>
        Cannot connect to Atuin Hub. Ensure your Internet connection is good, or try again later.
      </p>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Close</Button>
      </ModalFooter>
    );
  } else if (!ready) {
    body = (
      <div className="flex flex-col items-center justify-center gap-2">
        <Spinner />
        <p>Loading runbook information...</p>
      </div>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Cancel</Button>
      </ModalFooter>
    );
  } else {
    body = (
      <>
        <p>
          To open the runbook <strong>{remoteRunbookQuery.data?.name}</strong>@
          <strong>{props.tag}</strong>, you need to import it into a workspace. Choose a workspace
          below to import it into.
        </p>
        <WorkspaceSelector
          workspaces={workspaces.data ?? []}
          selectedWorkspace={selectedWorkspace}
          onSelect={setSelectedWorkspaceId}
        />
      </>
    );
    footer = (
      <ModalFooter>
        <Button onPress={handleClose}>Cancel</Button>
        <Button
          onPress={confirmImportRunbook}
          color="primary"
          isDisabled={failed || !ready || !selectedWorkspaceId || importing || cannotImport}
          isLoading={importing}
        >
          Import
        </Button>
      </ModalFooter>
    );
  }

  return (
    <Modal isOpen onClose={handleClose}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader>
              {step === "forked-options" ? "Open Runbook" : "Import Runbook"}
            </ModalHeader>
            <ModalBody>{body}</ModalBody>
            {footer}
          </>
        )}
      </ModalContent>
    </Modal>
  );
}

interface WorkspaceSelectorProps {
  workspaces: Workspace[];
  selectedWorkspace: Workspace | null;
  onSelect: (workspaceId: string) => void;
}

function defaultWorkspaceSelectorState(
  selectedWorkspaceId: string | null,
): Record<string, boolean> {
  if (!selectedWorkspaceId) {
    return {};
  }

  return {
    [selectedWorkspaceId]: true,
  };
}

function collapseReducer(state: Record<string, boolean>, action: { type: "toggle"; id: string }) {
  return {
    ...state,
    [action.id]: !state[action.id],
  };
}

function WorkspaceSelector(props: WorkspaceSelectorProps) {
  const orgs = useStore((state) => state.userOrgs);

  const [uncollapsed, dispatchCollapse] = useReducer(
    collapseReducer,
    defaultWorkspaceSelectorState(props.selectedWorkspace?.get("orgId") ?? "<PERSONAL>"),
  );

  const orgsDisplay = [
    { name: "Personal", id: "<PERSONAL>" },
    ...orgs.map((org) => ({ name: org.name, id: org.id })),
  ];
  const workspacesPerOrg = orgsDisplay.reduce((acc, org) => {
    acc[org.id] = props.workspaces.filter(
      (ws) =>
        ws.get("orgId") === (org.id == "<PERSONAL>" ? null : org.id) && ws.canManageRunbooks(),
    );
    return acc;
  }, {} as Record<string, Workspace[]>);

  useLayoutEffect(() => {
    if (props.selectedWorkspace) {
      const workspaceElement = document.getElementById(
        `import-workspace-selector-${props.selectedWorkspace.get("id")}`,
      );
      if (workspaceElement) {
        workspaceElement.scrollIntoView({ behavior: "smooth" });
      }
    }
  }, []);

  return (
    <div className="max-h-[200px] overflow-y-auto">
      {orgsDisplay.map((org) => (
        <ul key={org.id}>
          <p
            className="cursor-pointer font-bold"
            onClick={() => dispatchCollapse({ type: "toggle", id: org.id })}
          >
            <DynamicIcon
              name={uncollapsed[org.id] ? "chevron-down" : "chevron-right"}
              className="w-4 h-4 inline-block"
            />
            {org.name}
          </p>
          <ul className={cn("ml-4 mb-2", uncollapsed[org.id] ? "block" : "hidden")}>
            {workspacesPerOrg[org.id]?.map((ws) => (
              <li
                key={ws.get("id")}
                id={`import-workspace-selector-${ws.get("id")}`}
                className={cn(
                  "cursor-pointer",
                  props.selectedWorkspace?.get("id") === ws.get("id") ? "text-blue-500" : "",
                )}
                onClick={() => props.onSelect(ws.get("id")!)}
              >
                {ws.get("name")}
              </li>
            ))}
          </ul>
        </ul>
      ))}
    </div>
  );
}
