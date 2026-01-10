import { PauseIcon, PlayIcon } from "lucide-react";
import { Input, Button, Select, SelectItem } from "@heroui/react";
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";
import { exportPropMatter } from "@/lib/utils";
import { useSerialExecution } from "@/lib/hooks/useSerialExecution";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import track_event from "@/tracking";

interface PauseProps {
  id: string;
  condition: string;
  pauseIfTruthy: boolean;
  isEditable: boolean;
  onConditionChange: (condition: string) => void;
  onPauseIfTruthyChange: (pauseIfTruthy: boolean) => void;
}

const Pause = ({
  id,
  condition,
  pauseIfTruthy,
  isEditable,
  onConditionChange,
  onPauseIfTruthyChange,
}: PauseProps) => {
  const runbookId = useCurrentRunbookId();
  const serialExecution = useSerialExecution(runbookId);

  const isPausedAtThisBlock = serialExecution.isPaused && serialExecution.pausedAtBlockId === id;

  const handleContinue = () => {
    serialExecution.resumeFrom(id);
    track_event("runbooks.block.pause.continue", { block_id: id });
  };

  const handleModeChange = (keys: any) => {
    const key = keys.currentKey;
    onPauseIfTruthyChange(key === "conditional");
  };

  return (
    <div className="flex flex-col w-full bg-gradient-to-r from-amber-50 to-orange-50 dark:from-slate-800 dark:to-amber-950 rounded-lg p-3 border border-amber-200 dark:border-amber-900 shadow-sm hover:shadow-md transition-all duration-200">
      <div className="flex flex-row items-center gap-2">
        <div className="flex items-center">
          {isPausedAtThisBlock ? (
            <Button
              isIconOnly
              variant="solid"
              color="success"
              size="sm"
              onPress={handleContinue}
              className="animate-pulse-glow"
            >
              <PlayIcon className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              isIconOnly
              variant="light"
              size="sm"
              className="bg-amber-100 dark:bg-amber-800 text-amber-600 dark:text-amber-300"
            >
              <PauseIcon className="h-4 w-4" />
            </Button>
          )}
        </div>

        <span className="text-xs font-medium text-amber-700 dark:text-amber-300">Pause</span>

        <Select
          size="sm"
          className="w-40"
          selectedKeys={[pauseIfTruthy ? "conditional" : "always"]}
          onSelectionChange={handleModeChange}
          isDisabled={!isEditable}
          aria-label="Pause mode"
          classNames={{
            trigger: "h-8 min-h-8",
          }}
        >
          <SelectItem key="always">Always</SelectItem>
          <SelectItem key="conditional">If condition</SelectItem>
        </Select>

        {pauseIfTruthy && (
          <Input
            placeholder="e.g. {{ var.should_pause }}"
            value={condition}
            onValueChange={onConditionChange}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck="false"
            size="sm"
            className="flex-1 font-mono text-sm"
            isDisabled={!isEditable}
            classNames={{
              inputWrapper: "h-8 min-h-8",
            }}
          />
        )}
      </div>
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "pause",
    propSchema: {
      condition: { default: "" },
      pauseIfTruthy: { default: false },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      const propMatter = exportPropMatter("pause", block.props, [
        "label",
        "condition",
        "pauseIfTruthy",
      ]);
      return (
        <pre lang="pause">
          <code>{propMatter}</code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onConditionChange = (condition: string): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, condition },
        });
      };

      const onPauseIfTruthyChange = (pauseIfTruthy: boolean): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, pauseIfTruthy },
        });
      };

      return (
        <Pause
          id={block.id}
          condition={block.props.condition}
          pauseIfTruthy={block.props.pauseIfTruthy}
          isEditable={editor.isEditable}
          onConditionChange={onConditionChange}
          onPauseIfTruthyChange={onPauseIfTruthyChange}
        />
      );
    },
  },
);

// Component to insert this block from the editor menu
export const insertPause = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Pause",
  subtext: "Pause workflow execution for manual intervention",
  onItemClick: async () => {
    track_event("runbooks.block.create", { type: "pause" });

    editor.insertBlocks(
      [
        {
          type: "pause",
          props: {},
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <PauseIcon size={18} />,
  group: "Execute",
});

AIBlockRegistry.getInstance().addBlock({
  typeName: "pause",
  friendlyName: "Pause",
  shortDescription:
    "Pauses workflow execution until the user continues.",
  description: undent`
    Pause blocks halt serial workflow execution until the user manually continues. Can be unconditional or conditional based on a template variable.

    The available props are:
    - condition (string): A template expression to evaluate (only used when pauseIfTruthy is true)
    - pauseIfTruthy (boolean): If true, only pauses when condition evaluates to a truthy value

    TRUTHY VALUES:
    Truthy values include: true, "true", "1", "yes", or any non-zero number.
    All other values (including empty strings, "false", "0", "no") are considered falsy.

    Use pause blocks for manual checkpoints, confirmation steps, or conditional halts in automated workflows.

    Example: {
      "type": "pause",
      "props": {
        "pauseIfTruthy": true,
        "condition": "{{ var.needs_approval }}"
      }
    }
  `,
});
