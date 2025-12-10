import { useBlockNoteEditor, useComponentsContext, useExtensionState } from "@blocknote/react";
import { SideMenuExtension } from "@blocknote/core/extensions";
import { ClipboardCopyIcon } from "lucide-react";
import { useStore } from "@/state/store";
import { uuidv7 } from "uuidv7";

export function CopyBlockItem() {
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
      icon={<ClipboardCopyIcon size={16} />}
      onClick={() => {
        // This is the same workaround as in DuplicateBlockItem.
        let block = editor.getBlock(hoveredBlock.id) || hoveredBlock;
        block = structuredClone(block);
        block.id = uuidv7();
        useStore.getState().setCopiedBlock(block);
      }}
    >
      Copy
    </Components.Generic.Menu.Item>
  );
}

