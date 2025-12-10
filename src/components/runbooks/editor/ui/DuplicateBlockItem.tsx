import { useBlockNoteEditor, useComponentsContext, useExtensionState } from "@blocknote/react";
import { SideMenuExtension } from "@blocknote/core/extensions";
import { CopyIcon } from "lucide-react";
import { uuidv7 } from "uuidv7";

export function DuplicateBlockItem() {
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
      icon={<CopyIcon size={16} />}
      onClick={() => {
        // HACK [mkt]: For some blocks using CodeMirror, it seems that `props.block`
        // is missing the code in its props. However, `editor.document` has the correct
        // value in the block's props. So, we use `editor.document` to get the block.
        let block = editor.getBlock(hoveredBlock.id) || hoveredBlock;
        block = structuredClone(block);

        let id = uuidv7();
        block.id = id;

        editor.insertBlocks([block as any], hoveredBlock.id, "after");
      }}
    >
      Duplicate
    </Components.Generic.Menu.Item>
  );
}
