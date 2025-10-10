import {
  DragHandleMenuProps,
  useBlockNoteEditor,
  useComponentsContext,
} from "@blocknote/react";
import { uuidv7 } from "uuidv7";

export function DuplicateBlockItem(props: DragHandleMenuProps) {
  const editor = useBlockNoteEditor();

  const Components = useComponentsContext()!;

  return (
    <Components.Generic.Menu.Item
      onClick={() => {
        // HACK [mkt]: For some blocks using CodeMirror, it seems that `props.block`
        // is missing the code in its props. However, `editor.document` has the correct
        // value in the block's props. So, we use `editor.document` to get the block.
        let block = editor.getBlock(props.block.id) || props.block;
        block = structuredClone(block);

        let id = uuidv7();
        block.id = id;

        editor.insertBlocks([block], props.block.id, "after");
      }}
    >
      Duplicate
    </Components.Generic.Menu.Item>
  );
}
