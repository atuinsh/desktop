import { useEffect, useState, useCallback, useContext } from "react";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogPortal, DialogTitle } from "@/components/ui/dialog";

import { type SearchResultItem } from "./data";

import { AtuinState, useStore } from "@/state/store";
import { NotebookIcon } from "lucide-react";
import RunbookIndexService from "@/state/runbooks/search";
import { useQuery } from "@tanstack/react-query";
import { allRunbooks } from "@/lib/queries/runbooks";
import { allWorkspaces } from "@/lib/queries/workspaces";
import RunbookContext from "@/context/runbook_context";
import { VisuallyHidden } from "@heroui/react";

interface CommandMenuProps {
  index: RunbookIndexService;
}

export default function CommandMenu(props: CommandMenuProps) {
  const [query, setQuery] = useState("");
  const { data: runbooks } = useQuery(allRunbooks());
  const { data: workspaces } = useQuery(allWorkspaces());
  const isOpen = useStore((store: AtuinState) => store.searchOpen);
  const setOpen = useStore((store: AtuinState) => store.setSearchOpen);
  const currentWorkspaceId = useStore((store: AtuinState) => store.currentWorkspaceId);
  const setCurrentWorkspaceId = useStore((store: AtuinState) => store.setCurrentWorkspaceId);
  const [results, setResults] = useState<SearchResultItem[]>([]);
  const { activateRunbook } = useContext(RunbookContext);

  const onClose = useCallback(() => {
    setOpen(false);
  }, [setOpen]);

  const onOpen = useCallback(() => {
    setOpen(true);
  }, [setOpen]);

  useEffect(() => {
    let cancelled = false;
    if (query.length < 2) setResults([]);

    (async () => {
      let matches = await props.index.searchRunbooks(query);
      let res: (SearchResultItem | null)[] = matches.map((id) => {
        let rb = (runbooks || []).find((runbook) => runbook.id === id);

        if (!rb) return null;

        return {
          id: rb.id,
          workspaceId: rb.workspaceId,
          workspaceName:
            (workspaces || []).find((w) => w.get("id") === rb.workspaceId)?.get("name") || null,
          title: rb.name,
          type: "runbook",
          subtitle: "edited xyz ago",
        };
      });

      if (!cancelled) setResults(res.filter((r) => r !== null));
    })();

    return () => {
      cancelled = true;
    };
  }, [query]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const isMac = navigator.userAgent.toLowerCase().includes("mac");
      const hotkey = isMac ? "metaKey" : "ctrlKey";

      if (e?.key?.toLowerCase() === "p" && e[hotkey] && !e.shiftKey) {
        e.preventDefault();
        isOpen ? onClose() : onOpen();
      }
    };

    document.addEventListener("keydown", onKeyDown);

    return () => {
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [isOpen, onOpen, onClose]);

  const onItemSelect = useCallback(
    (item: SearchResultItem) => {
      onClose();
      setQuery("");
      if (item.workspaceId) {
        setCurrentWorkspaceId(item.workspaceId);
      }
      activateRunbook(item.id);
    },
    [onClose, setCurrentWorkspaceId, activateRunbook],
  );

  const renderSearchItem = useCallback(
    (item: SearchResultItem) => (
      <CommandItem
        key={item.id}
        value={item.id}
        onSelect={() => onItemSelect(item)}
        className="flex items-center gap-2"
      >
        <NotebookIcon className="h-4 w-4" />
        <div className="flex-1">
          <div className="font-medium">{item.title}</div>
          {item.workspaceId !== currentWorkspaceId && (
            <div className="text-sm text-muted-foreground">{item.workspaceName}</div>
          )}
        </div>
      </CommandItem>
    ),
    [currentWorkspaceId, onItemSelect],
  );

  function handleOpenChange(open: boolean) {
    if (!open) {
      setQuery("");
    }
    setOpen(open);
  }

  return (
    <Dialog
      modal={false}
      open={isOpen}
      onOpenChange={handleOpenChange}
    >
      <DialogPortal>
        <DialogContent className="overflow-hidden p-0 data-[state=open]:animate-none data-[state=closed]:animate-none">
          <VisuallyHidden>
            <DialogTitle>Search Runbooks</DialogTitle>
          </VisuallyHidden>
          <Command shouldFilter={false}>
            <CommandInput placeholder="Search Runbooks..." value={query} onValueChange={setQuery} />
            <CommandList>
              <CommandEmpty>
                <div className="py-6 text-center text-sm">
                  {query.length === 0 ? (
                    <p>Type to search runbooks...</p>
                  ) : (
                    <div>
                      <p>No results for &quot;{query}&quot;</p>
                      <p className="text-muted-foreground">
                        {query.length === 1
                          ? "Try adding more characters to your search term."
                          : "Try searching for something else."}
                      </p>
                    </div>
                  )}
                </div>
              </CommandEmpty>
              {results.map((item) => renderSearchItem(item))}
            </CommandList>
          </Command>
        </DialogContent>
      </DialogPortal>
    </Dialog>
  );
}
