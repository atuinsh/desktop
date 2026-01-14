import { useSerialExecution } from "@/lib/hooks/useSerialExecution";
import { cn, useDebounce } from "@/lib/utils";
import {
  Button,
  Divider,
  Input,
  Popover,
  PopoverContent,
  PopoverTrigger,
  ScrollShadow,
} from "@heroui/react";
import clsx from "clsx";
import { CheckIcon, ChevronDownIcon } from "lucide-react";
import { ChangeEvent, useEffect, useMemo, useRef, useState } from "react";

function validateTagName(tag: string, existingTags: string[]) {
  if (tag.length < 2) {
    return "Tag must be at least 2 characters";
  }
  if (tag.length > 80) {
    return "Tag must be at most 80 characters";
  }
  if (tag === "latest") {
    return "Tag cannot be 'latest'";
  }
  if (!tag.match(TAG_REGEX)) {
    return "Tag can only contain letters, numbers, underscores, dashes, and periods";
  }
  if (existingTags.find((t) => t == tag)) {
    return "Tag already exists";
  }
  return null;
}

interface TagSelectorProps {
  runbookId: string;
  tags: { value: string; text: string }[];
  currentTag: string | null;
  canEditTags: boolean;
  isOpen: boolean;
  onTrigger: () => void;
  onClose: () => void;
  onSelectTag: (tag: string) => void;
  onCreateTag: (tag: string) => Promise<void>;
}

const TAG_REGEX = /^[a-z0-9_\-\.]+$/i;

export default function TagSelector(props: TagSelectorProps) {
  const [newTagName, setNewTagName] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [isDebounced, resetDebounce, clearDebounce] = useDebounce(1000);
  const tagMenuRef = useRef<HTMLDivElement>(null);
  const serialExecution = useSerialExecution(props.runbookId);

  let tag = props.currentTag;
  let currentTagNames = useMemo(() => {
    return props.tags.map((t) => t.text);
  }, [props.tags]);

  let tagLabel: string = "";
  if (tag == "latest") {
    tagLabel = "(no tag)";
  } else if (tag) {
    tagLabel = tag;
  }

  // Scroll to currently selected tag when menu opens
  useEffect(() => {
    if (props.isOpen && tagMenuRef.current) {
      const selectedNode = tagMenuRef.current.querySelector("[data-selected=true]");
      if (selectedNode) {
        selectedNode.scrollIntoView({ block: "center" });
      }
    }
  }, [props.isOpen]);

  // Reset state when runbook changes
  useEffect(() => {
    setNewTagName("");
    setError(null);
    clearDebounce();
  }, [props.runbookId]);

  // Update error state after the user stops typing
  useEffect(() => {
    if (isDebounced) {
      const error = validateTagName(newTagName, currentTagNames);
      if (error) {
        setError(error);
      } else {
        setError(null);
      }
    }
  }, [isDebounced]);

  function handleTagNameChange(e: ChangeEvent<HTMLInputElement>) {
    const value = e.currentTarget.value;
    setError(null);
    setNewTagName(value);
    if (value == "") {
      clearDebounce();
    } else {
      resetDebounce();
    }
  }

  async function handleFormSubmit(e: React.FormEvent) {
    e.preventDefault();
    try {
      await props.onCreateTag(newTagName);
      setNewTagName("");
      setError(null);
      clearDebounce();
    } catch (err: any) {
      setError(err.message);
    }
  }

  const renderTagSelector = () => {
    return (
      <>
        {props.tags.map(({ value, text }) => (
          <div
            key={value}
            className={cn(
              "flex flex-row justify-between hover:bg-gray-300 hover:cursor-pointer px-2 mb-1 py-1 rounded-md dark:hover:bg-content3",
              {
                "hover:cursor-pointer": !serialExecution.isRunning,
                "hover:cursor-not-allowed": serialExecution.isRunning,
              },
            )}
            onClick={() => props.onSelectTag(value)}
            data-selected={value == props.currentTag}
          >
            <span>{text}</span>
            <span className={cn({ hidden: value != props.currentTag })}>
              <CheckIcon size={20} />
            </span>
          </div>
        ))}
      </>
    );
  };

  return (
    <Popover
      showArrow
      placement="bottom-start"
      triggerType="menu"
      // Set opaque backdrop so that tooltipos don't show through from doc,
      // but then set the bg color to transparent so there's no coloring.
      backdrop="opaque"
      classNames={{
        backdrop: "bg-transparent",
        content: "w-[350px] p-4 grow-1",
      }}
      isOpen={props.isOpen}
      shouldCloseOnInteractOutside={() => true}
      onClose={props.onClose}
    >
      <PopoverTrigger onClick={props.onTrigger}>
        <div
          className={clsx(
            "flex items-center max-w-[200px] rounded px-1.5 py-0.5",
            "bg-transparent hover:bg-black/5 dark:hover:bg-white/5",
            "transition-colors duration-150 text-xs",
            "text-gray-500 dark:text-gray-400",
            {
              "cursor-pointer hover:text-gray-700 dark:hover:text-gray-300": !serialExecution.isRunning,
              "cursor-not-allowed opacity-50": serialExecution.isRunning,
            }
          )}
        >
          <span className="truncate">@ {tagLabel}</span>
          <ChevronDownIcon className="ml-1 shrink-0" size={12} />
        </div>
      </PopoverTrigger>
      <PopoverContent>
        <div>Select a tag to view a previously saved snapshot of this runbook</div>
        <ScrollShadow
          size={40}
          className="my-2 overflow-y-auto pr-2 w-full max-h-[30vh]"
          ref={tagMenuRef}
        >
          {renderTagSelector()}
        </ScrollShadow>
        {props.canEditTags && (
          <div>
            <Divider className="mb-2" />
            <div className="font-bold">Create a new tag</div>
            <div className="mb-2">
              Create a new tag to make a permanent snapshot of the runbook in its untagged state
            </div>
            <form onSubmit={handleFormSubmit}>
              <div className="flex flex-col">
                <Input
                  label="Tag name"
                  value={newTagName}
                  onChange={handleTagNameChange}
                  variant="bordered"
                  placeholder="Tag name"
                  autoComplete="off"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck="false"
                />
                {error && <div className="text-red-600 italic text-sm my-1">{error}</div>}
                <Button
                  type="submit"
                  size="sm"
                  variant="flat"
                  color="success"
                  className="mt-2"
                  isDisabled={!isDebounced || !!error}
                >
                  Create tag
                </Button>
              </div>
            </form>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
