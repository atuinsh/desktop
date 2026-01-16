import { useComponentsContext } from "@blocknote/react";
import { SparklesIcon } from "lucide-react";

interface AskAIMenuItemProps {
  onAskAI: () => void;
}

/**
 * Menu item for the block's drag handle menu that opens the block-local AI agent.
 */
export function AskAIMenuItem({ onAskAI }: AskAIMenuItemProps) {
  const Components = useComponentsContext();

  if (!Components) {
    return null;
  }

  return (
    <Components.Generic.Menu.Item onClick={onAskAI}>
      <Components.Generic.Menu.Label>
        <SparklesIcon className="h-4 w-4 mr-2 text-purple-600 dark:text-purple-400" />
        Ask AI
      </Components.Generic.Menu.Label>
    </Components.Generic.Menu.Item>
  );
}
