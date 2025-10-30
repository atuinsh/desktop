// Make a new SSH connection and set it as the current connection
// Connections are pooled globally, so if we are already connected to the host just reuse.

import { Button, Input, Tooltip } from "@heroui/react";
import { GlobeIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import track_event from "@/tracking";
import { exportPropMatter } from "@/lib/utils";

interface SshConnectProps {
  userHost: string; // foo@bar, combine for natural-ness
  onUserHostChange: (userHost: string) => void;
  isEditable: boolean;
}

const SshConnect = ({ userHost, onUserHostChange, isEditable }: SshConnectProps) => {

  return (
    <Tooltip
      content="Ensure we are connected to an SSH server and make it the current connection"
      delay={1000}
      className="outline-none"
    >
      <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-slate-50 to-gray-50 dark:from-slate-800 dark:to-slate-900 rounded-lg p-3 border border-slate-200 dark:border-slate-700 shadow-sm hover:shadow-md transition-all duration-200">
        <div className="flex items-center">
          <Button
            isIconOnly
            variant="light"
            className="bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300"
          >
            <GlobeIcon className="h-4 w-4" />
          </Button>
        </div>

        <div className="flex-1">
          <Input
            placeholder="myserver or user@host:port"
            value={userHost}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck="false"
            className="flex-1 border-slate-200 dark:border-slate-700 focus:ring-slate-500"
            onValueChange={onUserHostChange}
            disabled={!isEditable}
          />
        </div>
      </div>
    </Tooltip>
  );
};

export default createReactBlockSpec(
  {
    type: "ssh-connect",
    propSchema: {
      userHost: { default: "" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      return (
        <pre lang="ssh-connect">
          <code>
            {exportPropMatter("ssh-connect", block.props, ["userHost"])}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const onUserHostChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, userHost: val },
        });
      };

      return (
        <SshConnect
          userHost={block.props.userHost}
          onUserHostChange={onUserHostChange}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

export const insertSshConnect = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "SSH Connect",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "ssh-connect" });
    
    editor.insertBlocks(
      [
        {
          type: "ssh-connect",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <GlobeIcon size={18} />,
  group: "Network",
});
