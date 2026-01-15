import { useEffect, useState } from "react";
import { Input, Button } from "@heroui/react";
import { CloudOffIcon, LockIcon, EyeIcon, EyeOffIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";
import track_event from "@/tracking";
import { exportPropMatter } from "@/lib/utils";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";
import isValidVarName from "../../utils/varNames";

interface LocalVarProps {
  blockId: string;
  name: string;
  obscured: boolean;
  isEditable: boolean;
  onNameUpdate: (name: string) => void;
  onObscuredUpdate: (obscured: boolean) => void;
}

const LocalVar = (props: LocalVarProps) => {
  const [value, setValue] = useBlockKvValue(props.blockId, "value", "");

  const [hasNameError, setHasNameError] = useState(false);

  // Check for invalid variable name characters (only allow alphanumeric and underscore)
  useEffect(() => {
    setHasNameError(!isValidVarName(props.name));
  }, [props.name]);

  return (
    <div className="flex flex-col w-full bg-gradient-to-r from-purple-50 to-indigo-50 dark:from-slate-800 dark:to-purple-950 rounded-lg p-3 border border-purple-200 dark:border-purple-900 shadow-sm hover:shadow-md transition-all duration-200">
      <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500 mb-2">local-var</span>
      <div className="flex flex-row items-center space-x-3">
        <div className="flex items-center">
          <Button
            isIconOnly
            variant="light"
            className="bg-purple-100 dark:bg-purple-800 text-purple-600 dark:text-purple-300"
          >
            <CloudOffIcon className="h-4 w-4" />
          </Button>
        </div>

        <Input
          placeholder="Name (shared)"
          value={props.name}
          onValueChange={props.onNameUpdate}
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck="false"
          className={`flex-1 ${
            hasNameError
              ? "border-red-400 dark:border-red-400 focus:ring-red-500"
              : "border-purple-200 dark:border-purple-800 focus:ring-purple-500"
          }`}
          disabled={!props.isEditable}
          isInvalid={hasNameError}
          errorMessage={"Variable names can only contain letters, numbers, and underscores"}
        />

        <Input
          placeholder="Value (private and ephemeral - only stored on your device)"
          value={value}
          onValueChange={setValue}
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck="false"
          className="flex-1 border-purple-200 dark:border-purple-800 focus:ring-purple-500"
          disabled={!props.isEditable}
          type={props.obscured ? "password" : "text"}
        />

        <Button
          isIconOnly
          variant="ghost"
          size="sm"
          className="text-gray-400 dark:text-gray-500 hover:text-purple-600 dark:hover:text-purple-300 min-w-6 w-6 h-6 border-none"
          onPress={() => props.onObscuredUpdate(!props.obscured)}
          isDisabled={!props.isEditable}
        >
          {props.obscured ? <EyeOffIcon className="h-3.5 w-3.5" /> : <EyeIcon className="h-3.5 w-3.5" />}
        </Button>
      </div>
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "local-var",
    propSchema: {
      name: { default: "" },
      // No value stored in props - only the key/name is synced
      // obscured defaults to true for backward compatibility with existing blocks
      obscured: { default: true },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("local-var", block.props, ["name", "obscured"]);
      return (
        <pre lang="local-var">
          <code>{propMatter}</code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onNameUpdate = (name: string): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name },
        });
      };

      const onObscuredUpdate = (obscured: boolean): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, obscured },
        });
      };

      return (
        <LocalVar
          blockId={block.id}
          name={block.props.name}
          obscured={block.props.obscured}
          onNameUpdate={onNameUpdate}
          onObscuredUpdate={onObscuredUpdate}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

// Component to insert this block from the editor menu
export const insertLocalVar = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Local Variable",
  subtext: "Variable stored only on your device",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "local-var" });

    editor.insertBlocks(
      [
        {
          type: "local-var",
          props: {
            // New blocks default to not obscured for better UX
            obscured: false,
          },
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <LockIcon size={18} />,
  group: "Execute", // Match the group of regular var component
});

AIBlockRegistry.getInstance().addBlock({
  typeName: "local-var",
  friendlyName: "Local Variable",
  shortDescription:
    "Stores a variable locally on the user's device (not synced).",
  description: undent`
    Local Variable blocks store sensitive values locally on the user's machine. The variable name is synced with collaborators, but the value is stored only on the local device and never uploaded.

    The available props are:
    - name (string): The variable name (synced with collaborators)

    The value is stored locally and can be referenced using {{ var.variable_name }} syntax. This is ideal for credentials, API keys, or other sensitive data that shouldn't be shared.

    IMPORTANT: This block's value is UI-only - the user must manually enter the value in the runbook editor. You cannot set the value programmatically via props.

    Use case: Add this block to let users enter credentials, paths, or other sensitive data that shouldn't be stored in the runbook document.

    Example: {
      "type": "local-var",
      "props": {
        "name": "api_token"
      }
    }
  `,
});
