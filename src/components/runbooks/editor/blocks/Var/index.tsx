import React, { useEffect, useState } from "react";
import { Input, Button } from "@heroui/react";
import { TextCursorInputIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";
import { exportPropMatter } from "@/lib/utils";
import isValidVarName from "../../utils/varNames";

interface VarProps {
  name: string;
  value: string;
  isEditable: boolean;
  onUpdate: (name: string, value: string) => void;
}

const Var = ({ name = "", value = "", onUpdate, isEditable }: VarProps) => {
  const [hasNameError, setHasNameError] = useState(false);

  // Check for invalid variable name characters (only allow alphanumeric and underscore)
  useEffect(() => {
    setHasNameError(!isValidVarName(name));
  }, [name]);

  const handleKeyChange = (e: React.FormEvent<HTMLInputElement>) => {
    const newName = e.currentTarget.value;
    onUpdate(newName, value);
  };

  const handleValueChange = (e: React.FormEvent<HTMLInputElement>) => {
    onUpdate(name, e.currentTarget.value);
  };

  return (
    <div className="flex flex-col w-full bg-gradient-to-r from-green-50 to-emerald-50 dark:from-slate-800 dark:to-emerald-950 rounded-lg p-3 border border-green-200 dark:border-green-900 shadow-sm hover:shadow-md transition-all duration-200">
      <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500 mb-2">var</span>
      <div className="flex flex-row items-center space-x-3">
        <div className="flex items-center">
          <Button
            isIconOnly
            variant="light"
            className="bg-green-100 dark:bg-green-800 text-green-600 dark:text-green-300"
          >
            <TextCursorInputIcon className="h-4 w-4" />
          </Button>
        </div>

        <Input
          placeholder="Name"
          value={name}
          onChange={handleKeyChange}
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck="false"
          className={`flex-1 ${
            hasNameError
              ? "border-red-400 dark:border-red-400 focus:ring-red-500"
              : "border-green-200 dark:border-green-800 focus:ring-green-500"
          }`}
          disabled={!isEditable}
          isInvalid={hasNameError}
          errorMessage={"Variable names can only contain letters, numbers, and underscores"}
        />

        <div className="flex-1">
          <Input
            placeholder="Value"
            value={value}
            onChange={handleValueChange}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck="false"
            className="flex-1 border-green-200 dark:border-green-800 focus:ring-green-500"
            disabled={!isEditable}
          />
        </div>
      </div>
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "var",
    propSchema: {
      name: { default: "" },
      value: { default: "" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("var", block.props, ["name"]);
      return (
        <pre lang="var">
          <code>
            {propMatter}
            {block.props.value}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onUpdate = (name: string, value: string): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name: name, value: value },
        });
      };

      return (
        <Var
          name={block.props.name}
          value={block.props.value}
          onUpdate={onUpdate}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

AIBlockRegistry.getInstance().addBlock({
  typeName: "var",
  friendlyName: "Template Variable",
  shortDescription: "Sets a template variable for use in subsequent blocks.",
  description: undent`
    Template Variable blocks define variables that can be referenced in other blocks using the {{ var.variable_name }} syntax. Values are synced with collaborators.
    For sensitive data, or values that should not be synced, use a Local Variable block instead.

    The available props are:
    - name (string): The variable name (alphanumeric and underscores only)
    - value (string): The variable value

    Template variables are useful for parameterizing runbooks with values that may change between runs or environments.

    Example: {
      "type": "var",
      "props": {
        "name": "api_base_url",
        "value": "https://api.example.com"
      }
    }
  `,
});
