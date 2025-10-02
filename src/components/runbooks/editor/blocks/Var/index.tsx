import React, { useEffect, useMemo, useState } from "react";
import { Input, Button, addToast } from "@heroui/react";
import { TextCursorInputIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import {
  getDependentVariables,
  setTemplateVar,
  TemplateErrorBehavior,
  templateString,
} from "@/state/templates";
import { exportPropMatter } from "@/lib/utils";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import RunbookBus from "@/lib/app/runbook_bus";

interface VarProps {
  id: string;
  runbookId: string | null;
  name: string;
  value: string;
  dependentVariables: string[];
  isEditable: boolean;
  onUpdate: (name: string, value: string) => void;
}

const Var = ({
  id,
  runbookId,
  name = "",
  value = "",
  dependentVariables,
  onUpdate,
  isEditable,
}: VarProps) => {
  const hasNameError = useMemo(() => !!(name && !/^[a-zA-Z0-9_]*$/.test(name)), [name]);
  const hasDependencyError = useMemo(
    () => !!(name && dependentVariables.includes(name)),
    [name, dependentVariables],
  );

  const handleKeyChange = (newName: string) => {
    onUpdate(newName, value);
  };

  const handleValueChange = (newValue: string) => {
    onUpdate(name, newValue);
  };

  useEffect(() => {
    if (!runbookId) return;

    const bus = RunbookBus.get(runbookId);
    return bus.onVariableChanged((changedName, _changedValue, source) => {
      if (source !== id && dependentVariables.includes(changedName)) {
        handleValueChange(value);
      }
    });
  }, [name, runbookId, value, dependentVariables]);

  let errorMessage = hasNameError
    ? "Variable names can only contain letters, numbers, and underscores"
    : hasDependencyError
    ? "Variable is dependent on itself"
    : null;

  return (
    <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-green-50 to-emerald-50 dark:from-slate-800 dark:to-emerald-950 rounded-lg p-3 border border-green-200 dark:border-green-900 shadow-sm hover:shadow-md transition-all duration-200">
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
        onValueChange={handleKeyChange}
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck="false"
        className={`flex-1 ${
          hasNameError || hasDependencyError
            ? "border-red-400 dark:border-red-400 focus:ring-red-500"
            : "border-green-200 dark:border-green-800 focus:ring-green-500"
        }`}
        disabled={!isEditable}
        isInvalid={hasNameError || hasDependencyError}
        errorMessage={errorMessage}
      />

      <div className="flex-1">
        <Input
          placeholder="Value"
          value={value}
          onValueChange={handleValueChange}
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

      const [dependentVariables, setDependentVariables] = useState<string[]>([]);
      useEffect(() => {
        (async () => {
          let dependent = await getDependentVariables(block.props.value);
          dependent = dependent.map((v) => {
            return v.startsWith("var.") ? v.slice(4) : v;
          });
          setDependentVariables(dependent);
        })();
      }, [block.props.value]);

      const onUpdate = (name: string, value: string): void => {
        const id = block.id;

        // First update the block props
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, name: name, value: value },
        });

        // Then update the template variable in the backend state
        if (name && currentRunbookId) {
          templateString(
            id,
            value,
            editor.document,
            currentRunbookId,
            TemplateErrorBehavior.SUPPRESS_ERROR,
          ).then((value) => {
            setTemplateVar(currentRunbookId, name, value, id).catch(console.error);
          });
        }
      };

      return (
        <Var
          id={block.id}
          runbookId={currentRunbookId}
          name={block.props.name}
          value={block.props.value}
          dependentVariables={dependentVariables}
          onUpdate={onUpdate}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);
