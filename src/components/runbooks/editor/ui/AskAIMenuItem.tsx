import { useBlockNoteEditor, useComponentsContext, useExtensionState } from "@blocknote/react";
import { SideMenuExtension } from "@blocknote/core/extensions";
import { SparklesIcon } from "lucide-react";

interface AskAIMenuItemProps {
  onAskAI: (blockId: string, blockType: string) => void;
}

/**
 * Menu item for the block's drag handle menu that opens the block-local AI agent.
 */
export function AskAIMenuItem({ onAskAI }: AskAIMenuItemProps) {
  const editor = useBlockNoteEditor();
  const Components = useComponentsContext()!;
  const hoveredBlock = useExtensionState(SideMenuExtension, {
    editor,
    selector: (state) => state?.block,
  });

  if (!hoveredBlock) {
    return null;
  }

  return (
    <Components.Generic.Menu.Item
      icon={<SparklesIcon size={16} className="text-purple-600 dark:text-purple-400" />}
      onClick={() => {
        onAskAI(hoveredBlock.id, hoveredBlock.type);
      }}
    >
      Ask AI
    </Components.Generic.Menu.Item>
  );
}
