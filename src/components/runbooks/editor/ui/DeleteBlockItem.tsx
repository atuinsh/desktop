import { DragHandleMenuProps, useBlockNoteEditor, useComponentsContext } from "@blocknote/react";
import { TrashIcon } from "lucide-react";

// Custom Side Menu button to remove the hovered block.
export function DeleteBlockItem(props: DragHandleMenuProps) {
  const editor = useBlockNoteEditor();

  const Components = useComponentsContext()!;

  return (
    <Components.Generic.Menu.Item
      icon={<TrashIcon size={16} />}
      onClick={() => {
        editor.removeBlocks([props.block]);
      }}
    >
      Delete
    </Components.Generic.Menu.Item>
  );
}
