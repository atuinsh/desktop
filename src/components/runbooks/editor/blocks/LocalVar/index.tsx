import React, { useState, useEffect } from "react";
import { Input, Button } from "@heroui/react";
import { CloudOffIcon, LockIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import { invoke } from "@tauri-apps/api/core";
import track_event from "@/tracking";
import { setTemplateVar } from "@/state/templates";
import { exportPropMatter } from "@/lib/utils";
import { useCurrentRunbookId } from "@/context/runbook_id_context";

interface LocalVarProps {
  name: string;
  isEditable: boolean;
  onNameUpdate: (name: string) => void;
}

const LocalVar = ({ name = "", onNameUpdate, isEditable }: LocalVarProps) => {
  // Store the value in local React state only, not in props
  // This ensures it doesn't sync to the remote document
  const [localValue, setLocalValue] = useState<string>("");
  const currentRunbookId = useCurrentRunbookId();

  // Get the initial value from backend when component mounts or name changes
  useEffect(() => {
    if (name && currentRunbookId) {
      invoke("get_template_var", {
        runbook: currentRunbookId,
        name: name,
      })
        .then((value: any) => {
          if (value) {
            setLocalValue(value as string);
          }
        })
        .catch(console.error);
    }
  }, [name, currentRunbookId]);

  const [hasNameError, setHasNameError] = useState(false);

  // Check for invalid variable name characters (only allow alphanumeric and underscore)
  useEffect(() => {
    const validNamePattern = /^[a-zA-Z0-9_]*$/;
    setHasNameError(!validNamePattern.test(name));
  }, [name]);

  const handleKeyChange = (e: React.FormEvent<HTMLInputElement>) => {
    const newName = e.currentTarget.value;
    
    // If name is changing and we have a current runbook
    if (name && name !== newName && currentRunbookId) {
      // First, get the current value
      invoke("get_template_var", {
        runbook: currentRunbookId,
        name: name,
      })
        .then((value: any) => {
          if (value && newName) {
            // Save under the new name
            setTemplateVar(currentRunbookId, newName, value as string);
          }
        })
        .catch(console.error);
    }

    onNameUpdate(newName);
  };

  const handleValueChange = (e: React.FormEvent<HTMLInputElement>) => {
    const newValue = e.currentTarget.value;
    setLocalValue(newValue);

    // Only save to backend if we have a name and runbook
    if (name && currentRunbookId) {
      setTemplateVar(currentRunbookId, name, newValue);
    }
  };

  return (
    <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-purple-50 to-indigo-50 dark:from-slate-800 dark:to-purple-950 rounded-lg p-3 border border-purple-200 dark:border-purple-900 shadow-sm hover:shadow-md transition-all duration-200">
        <div className="flex items-center">
          <Button isIconOnly variant="light" className="bg-purple-100 dark:bg-purple-800 text-purple-600 dark:text-purple-300">
            <CloudOffIcon className="h-4 w-4" />
          </Button>
        </div>

            <Input
              placeholder="Name (shared)"
              value={name}
              onChange={handleKeyChange}
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck="false"
              className={`flex-1 ${hasNameError ? 'border-red-400 dark:border-red-400 focus:ring-red-500' : 'border-purple-200 dark:border-purple-800 focus:ring-purple-500'}`}
              disabled={!isEditable}
              isInvalid={hasNameError}
              errorMessage={"Variable names can only contain letters, numbers, and underscores"}
            />

          <Input
            placeholder="Value (private and ephemeral - only stored on your device)"
            value={localValue}
            onChange={handleValueChange}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck="false"
            className="flex-1 border-purple-200 dark:border-purple-800 focus:ring-purple-500"
            disabled={!isEditable}
            type="password"
          />
      </div>
  );
};

export default createReactBlockSpec(
  {
    type: "local-var",
    propSchema: {
      name: { default: "" },
      // No value stored in props - only the key/name is synced
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("local-var", block.props, ["name"]);
      return (
        <pre lang="local-var">
          <code>
            {propMatter}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onNameUpdate = (name: string): void => {
        // Only update the name in the block props
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name },
        });
      };

      return (
        <LocalVar
          name={block.props.name}
          onNameUpdate={onNameUpdate}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

// Component to insert this block from the editor menu
export const insertLocalVar = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Local Variable",
  subtext: "Variable stored only on your device - useful for credentials",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "local-var" });

    editor.insertBlocks(
      [
        {
          type: "local-var",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <LockIcon size={18} />,
  group: "Execute", // Match the group of regular var component
}); 