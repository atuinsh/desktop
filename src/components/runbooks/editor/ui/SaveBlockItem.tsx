import { useBlockNoteEditor, useComponentsContext, useExtensionState } from "@blocknote/react";
import { SideMenuExtension } from "@blocknote/core/extensions";
import { SaveIcon } from "lucide-react";
import { useStore } from "@/state/store";

export function SaveBlockItem() {
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
      icon={<SaveIcon size={16} />}
      onClick={() => {
        // HACK [mkt]: For some blocks using CodeMirror, it seems that `props.block`
        // is missing the code in its props. However, `editor.document` has the correct
        // value in the block's props. So, we use `editor.document` to get the block.
        //
        // This is the same workaround as in DuplicateBlockItem.
        let block = editor.getBlock(hoveredBlock.id) || hoveredBlock;
        block = structuredClone(block);

        useStore.getState().setSavingBlock(block);
      }}
    >
      Save Block
    </Components.Generic.Menu.Item>
  );
}
