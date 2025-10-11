import { DragHandleMenuProps, useBlockNoteEditor, useComponentsContext } from "@blocknote/react";
import { SaveIcon } from "lucide-react";
import { useStore } from "@/state/store";

export function SaveBlockItem(props: DragHandleMenuProps) {
  const editor = useBlockNoteEditor();
  const Components = useComponentsContext()!;

  return (
    <Components.Generic.Menu.Item
      icon={<SaveIcon size={16} />}
      onClick={() => {
        // HACK [mkt]: For some blocks using CodeMirror, it seems that `props.block`
        // is missing the code in its props. However, `editor.document` has the correct
        // value in the block's props. So, we use `editor.document` to get the block.
        //
        // This is the same workaround as in DuplicateBlockItem.
        let block = editor.getBlock(props.block.id) || props.block;
        block = structuredClone(block);

        useStore.getState().setSavingBlock(block);
      }}
    >
      Save Block
    </Components.Generic.Menu.Item>
  );
}
