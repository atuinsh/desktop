import React, { useEffect, useState } from "react";
import { Input, Button } from "@heroui/react";
import { TextCursorInputIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import { setTemplateVar } from "@/state/templates";
import { exportPropMatter } from "@/lib/utils";
import { useCurrentRunbookId } from "@/context/runbook_id_context";

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
    const validNamePattern = /^[a-zA-Z0-9_]*$/;
    setHasNameError(!validNamePattern.test(name));
  }, [name]);

  const handleKeyChange = (e: React.FormEvent<HTMLInputElement>) => {
    const newName = e.currentTarget.value;
    onUpdate(newName, value);
  };

  const handleValueChange = (e: React.FormEvent<HTMLInputElement>) => {
    onUpdate(name, e.currentTarget.value);
  };

  return (
    <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-green-50 to-emerald-50 dark:from-slate-800 dark:to-emerald-950 rounded-lg p-3 border border-green-200 dark:border-green-900 shadow-sm hover:shadow-md transition-all duration-200">
        <div className="flex items-center">
          <Button isIconOnly variant="light" className="bg-green-100 dark:bg-green-800 text-green-600 dark:text-green-300">
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
              className={`flex-1 ${hasNameError ? 'border-red-400 dark:border-red-400 focus:ring-red-500' : 'border-green-200 dark:border-green-800 focus:ring-green-500'}`}
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
      const currentRunbookId = useCurrentRunbookId();

      const onUpdate = (name: string, value: string): void => {
        // First update the block props
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name: name, value: value },
        });

        // Then update the template variable in the backend state
        if (name && currentRunbookId) {
          setTemplateVar(currentRunbookId, name, value).catch(console.error);
        }
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