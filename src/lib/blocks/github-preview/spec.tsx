// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import GitHubPreview from "./components/GitHubPreview";
import { GITHUB_PREVIEW_BLOCK_SCHEMA, GitHubBlockProps } from "./schema";

export default createReactBlockSpec(GITHUB_PREVIEW_BLOCK_SCHEMA, {
  // @ts-ignore
  render: ({ block, editor }) => {
    const updateProps = (updates: Partial<GitHubBlockProps>) => {
      editor.updateBlock(block, {
        // @ts-ignore
        props: { ...block.props, ...updates },
      });
    };

    return (
      <div contentEditable={false} className="w-full">
        <GitHubPreview props={block.props} updateProps={updateProps} />
      </div>
    );
  },
});
