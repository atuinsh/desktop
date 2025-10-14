// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

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
);
