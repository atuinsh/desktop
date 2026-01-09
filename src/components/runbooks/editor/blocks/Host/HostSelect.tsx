// Specify which host commands should run on
// Currently only supports local execution

import { Button, Tooltip } from "@heroui/react";
import { HomeIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";
import track_event from "@/tracking";
import { exportPropMatter } from "@/lib/utils";

interface HostSelectProps {
  isEditable: boolean;
}

const HostSelect = ({ }: HostSelectProps) => {

  return (
    <Tooltip
      content="Specifies that commands run on the local machine"
      delay={1000}
      className="outline-none"
    >
      <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-amber-50 to-orange-50 dark:from-slate-800 dark:to-amber-950 rounded-lg p-3 border border-amber-200 dark:border-amber-900 shadow-sm hover:shadow-md transition-all duration-200">
        <div className="flex items-center">
          <Button isIconOnly variant="light" className="bg-amber-100 dark:bg-amber-800 text-amber-600 dark:text-amber-300">
            <HomeIcon className="h-4 w-4" />
          </Button>
        </div>
        
        <div className="flex-1">
          <Button 
            variant="flat"
            className="text-sm w-full justify-start border-amber-200 dark:border-amber-800"
            disabled={true} // Always disabled since we only support localhost
          >
            localhost
          </Button>
        </div>
      </div>
    </Tooltip>
  );
};

export default createReactBlockSpec(
  {
    type: "host-select",
    propSchema: {
      host: { default: "local" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      return (
        <pre lang="host-select">
          <code>
            {exportPropMatter("host-select", block.props, ["host"])}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      return (
        <HostSelect
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

export const insertHostSelect = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Host",
  subtext: "Specify that commands run on localhost",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "host-select" });

    editor.insertBlocks(
      [
        {
          type: "host-select",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <HomeIcon size={18} />,
  group: "Network",
});

AIBlockRegistry.getInstance().addBlock({
  typeName: "host-select",
  friendlyName: "Host",
  shortDescription:
    "Switches execution back to localhost.",
  description: undent`
    Host blocks specify that subsequent Terminal and Script blocks should run on the local machine. Use this after an SSH Connect block to switch back to local execution.

    The available props are:
    - host (string): Currently only supports "local"

    Example: {
      "type": "host-select",
      "props": {
        "host": "local"
      }
    }
  `,
}); 