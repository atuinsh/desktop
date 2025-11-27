import React from "react";
import { Input, Tooltip, Button } from "@heroui/react";
import { VariableIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import { exportPropMatter } from "@/lib/utils";

interface EnvProps {
  name: string;
  value: string;
  isEditable: boolean;
  onUpdate: (name: string, value: string) => void;
}

const Env = ({ name = "", value = "", onUpdate, isEditable }: EnvProps) => {
  const handleKeyChange = (e: React.FormEvent<HTMLInputElement>) => {
    onUpdate(e.currentTarget.value, value);
  };

  const handleValueChange = (e: React.FormEvent<HTMLInputElement>) => {
    onUpdate(name, e.currentTarget.value);
  };

  return (
    <Tooltip
      content="Set an environment variable for all subsequent code blocks"
      delay={1000}
      className="outline-none"
    >
      <div className="flex flex-col w-full bg-gradient-to-r from-green-50 to-emerald-50 dark:from-slate-800 dark:to-green-950 rounded-lg p-3 border border-green-200 dark:border-green-900 shadow-sm hover:shadow-md transition-all duration-200">
        <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500 mb-2">env</span>
        <div className="flex flex-row items-center space-x-3">
        <div className="flex items-center">
          <Button isIconOnly variant="light" className="bg-green-100 dark:bg-green-800 text-green-600 dark:text-green-300">
            <VariableIcon className="h-4 w-4" />
          </Button>
        </div>

        <div className="flex-1">
          <Input
            placeholder="Name"
            value={name}
            onChange={handleKeyChange}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck="false"
            className="flex-1 border-green-200 dark:border-green-800 focus:ring-green-500"
            disabled={!isEditable}
          />
        </div>

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
    </Tooltip>
  );
};

export default createReactBlockSpec(
  {
    type: "env",
    propSchema: {
      name: { default: "" },
      value: { default: "" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("env", block.props, ["name"]);
      return (
        <pre lang="env">
          <code>
            {propMatter}
            {block.props.value}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const onUpdate = (name: string, value: string): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name: name, value: value },
        });
      };

      return (
        <Env
          name={block.props.name}
          value={block.props.value}
          onUpdate={onUpdate}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);
