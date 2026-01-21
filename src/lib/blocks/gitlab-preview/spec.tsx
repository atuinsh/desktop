// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import GitLabPreview from "./components/GitLabPreview";
import { GITLAB_PREVIEW_BLOCK_SCHEMA, GitLabBlockProps } from "./schema";

export default createReactBlockSpec(GITLAB_PREVIEW_BLOCK_SCHEMA, {
  // @ts-ignore
  render: ({ block, editor }) => {
    const updateProps = (updates: Partial<GitLabBlockProps>) => {
      editor.updateBlock(block, {
        // @ts-ignore
        props: { ...block.props, ...updates },
      });
    };

    return (
      <div contentEditable={false} className="w-full">
        <GitLabPreview props={block.props} updateProps={updateProps} />
      </div>
    );
  },
});
