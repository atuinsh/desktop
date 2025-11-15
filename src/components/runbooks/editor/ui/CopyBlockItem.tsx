import { DragHandleMenuProps, useBlockNoteEditor, useComponentsContext } from "@blocknote/react";
import { ClipboardCopyIcon } from "lucide-react";
import { useStore } from "@/state/store";
import { uuidv7 } from "uuidv7";

export function CopyBlockItem(props: DragHandleMenuProps) {
  const editor = useBlockNoteEditor();
  const Components = useComponentsContext()!;

  return (
    <Components.Generic.Menu.Item
      icon={<ClipboardCopyIcon size={16} />}
      onClick={() => {
        // This is the same workaround as in DuplicateBlockItem.
        let block = editor.getBlock(props.block.id) || props.block;
        block = structuredClone(block);
        block.id = uuidv7();
        useStore.getState().setCopiedBlock(block);
      }}
    >
      Copy
    </Components.Generic.Menu.Item>
  );
}

