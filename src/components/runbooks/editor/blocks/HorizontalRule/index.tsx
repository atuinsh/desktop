import type { Extension } from "@blocknote/core";
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

/**
 * Simple horizontal rule component for visual separation
 */
const HorizontalRule = () => {
  return (
    <div className="w-full py-2">
      <hr className="border-t-2 border-gray-300 dark:border-gray-700" />
    </div>
  );
};

/**
 * BlockNote block specification for the HorizontalRule component
 * Renders a simple horizontal line for visual separation
 */
export default createReactBlockSpec(
  {
    type: "horizontal_rule",
    propSchema: {},
    content: "none",
  },
  {
    toExternalHTML: () => {
      return <hr />;
    },
    // @ts-ignore
    render: () => {
      return <HorizontalRule />;
    },
  },
  [
    {
      key: "horizontal-rule-shortcut",
      inputRules: [
        {
          find: new RegExp("^---$"),
          replace() {
            return { type: "horizontal_rule", props: {}, content: [] };
          },
        },
      ],
    } as Extension,
  ],
);

AIBlockRegistry.getInstance().addBlock({
  typeName: "horizontal_rule",
  friendlyName: "Horizontal Rule",
  shortDescription:
    "Inserts a visual separator line.",
  description: undent`
    Horizontal Rule blocks insert a visual separator line between sections of a runbook. This block has no configurable props.

    Can also be inserted by typing "---" on an empty line.

    Example: {
      "type": "horizontal_rule",
      "props": {}
    }
  `,
});
