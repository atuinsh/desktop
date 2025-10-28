import { Input, Tooltip, Button } from "@heroui/react";
import { FolderInputIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import { open } from "@tauri-apps/plugin-dialog";
import { exportPropMatter } from "@/lib/utils";

interface DirectoryProps {
  path: string;
  isEditable: boolean;
  onInputChange: (val: string) => void;
}

const Directory = ({ path, onInputChange, isEditable }: DirectoryProps) => {
  const selectFolder = async () => {
    if (isEditable) {
      const selectedPath = await open({
        multiple: false,
        directory: true,
      });

      onInputChange(selectedPath || "");
    }
  };

  return (
    <div className="w-full !max-w-full !outline-none overflow-none">
      <Tooltip
        content="Change working directory for all subsequent code blocks (shared with collaborators)"
        delay={1000}
      >
        <div className="flex flex-row items-center space-x-3 w-full bg-gradient-to-r from-blue-50 to-cyan-50 dark:from-slate-800 dark:to-blue-950 rounded-lg p-3 border border-blue-200 dark:border-blue-900 shadow-sm hover:shadow-md transition-all duration-200">
          <div className="flex items-center">
            <Button
              isIconOnly
              variant="light"
              className="bg-blue-100 dark:bg-blue-800 text-blue-600 dark:text-blue-300"
              aria-label="Select folder"
              onPress={selectFolder}
              disabled={!isEditable}
            >
              <FolderInputIcon className="h-4 w-4" />
            </Button>
          </div>

          <div className="flex-1">
            <Input
              placeholder="~ (working directory shared with collaborators)"
              value={path}
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck="false"
              onValueChange={onInputChange}
              disabled={!isEditable}
              className="flex-1 border-blue-200 dark:border-blue-800 focus:ring-blue-500"
            />
          </div>
        </div>
      </Tooltip>
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "directory",
    propSchema: {
      path: { default: "" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("directory", {}, []);
      return (
        <div>
          <pre lang="bash">
            <code>
              {propMatter}
              {block.props.path}
            </code>
          </pre>
        </div>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const onInputChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, path: val },
        });
      };

      return (
        <Directory
          path={block.props.path}
          onInputChange={onInputChange}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);
