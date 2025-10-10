import { DragHandleMenuProps, useComponentsContext } from "@blocknote/react";
import { SaveIcon } from "lucide-react";
import { useStore } from "@/state/store";

export function SaveBlockItem(props: DragHandleMenuProps) {
  const Components = useComponentsContext()!;

  return (
    <Components.Generic.Menu.Item
      icon={<SaveIcon size={16} />}
      onClick={() => {
        useStore.getState().setSavingBlock(props.block);
      }}
    >
      Save Block
    </Components.Generic.Menu.Item>
  );
}
