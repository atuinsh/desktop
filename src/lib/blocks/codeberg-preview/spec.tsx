// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import CodebergPreview from "./components/CodebergPreview";
import { CODEBERG_PREVIEW_BLOCK_SCHEMA, CodebergBlockProps } from "./schema";

export default createReactBlockSpec(CODEBERG_PREVIEW_BLOCK_SCHEMA, {
  // @ts-ignore
  render: ({ block, editor }) => {
    const updateProps = (updates: Partial<CodebergBlockProps>) => {
      editor.updateBlock(block, {
        // @ts-ignore
        props: { ...block.props, ...updates },
      });
    };

    return (
      <div contentEditable={false} className="w-full">
        <CodebergPreview props={block.props} updateProps={updateProps} />
      </div>
    );
  },
});
