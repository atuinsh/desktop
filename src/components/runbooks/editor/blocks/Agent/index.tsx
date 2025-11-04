import { createReactBlockSpec } from "@blocknote/react";
import { AgentBackend } from "./AgentBackend";

export default createReactBlockSpec(
  {
    type: "agent",
    propSchema: {},
    content: "none",
  },
  {
    render: ({ block, editor }) => {
      return (
        <AgentBackend
          id={block.id}
          editor={editor}
          isEditable={editor.isEditable}
          blockId={block.id}
        />
      );
    },
  },
);
