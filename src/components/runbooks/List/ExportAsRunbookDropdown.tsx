import Runbook from "@/state/runbooks/runbook";
import { Dropdown, DropdownTrigger, DropdownMenu, DropdownItem } from "@heroui/react";
import { save } from "@tauri-apps/plugin-dialog";

interface ExportRunbookDropdownProps {
  runbook: Runbook;
  isOpen: boolean;
  onClose: () => void;
}

export default function ExportRunbookDropdown({
  runbook,
  isOpen,
  onClose,
}: ExportRunbookDropdownProps) {
  let exportTypes = [
    /*
    {
      name: "Atuin Markdown",
      extension: "atmd",
      action: async () => {
        let filePath = await save({
          defaultPath: runbook.name + ".atmd",
        });

        if (!filePath) return;

        runbook?.exportMarkdown(filePath);
      },
    },
    */
    {
      name: "Atuin Runbook",
      extension: "atrb",
      action: async () => {
        let filePath = await save({
          defaultPath: runbook.name + ".atrb",
        });

        if (!filePath) return;

        // TODO
        // runbook?.export(filePath);
      },
    },
  ];

  return (
    <Dropdown
      isOpen={isOpen}
      placement="right-start"
      className="absolute left-[6.5rem] top-[-1rem]"
    >
      <DropdownTrigger title="Export as">
        <span className="w-full">Export as</span>
      </DropdownTrigger>
      <DropdownMenu
        aria-label="Workspace selection"
        variant="flat"
        topContent={<div className="text-default-600 font-semibold">Export</div>}
        items={exportTypes}
      >
        {(exportType) => {
          return (
            <DropdownItem
              key={exportType.name}
              textValue={exportType.name}
              className="py-2"
              onPress={async () => {
                if (!exportType.action) return;
                await exportType.action();

                onClose();
              }}
            >
              <div className="flex items-center gap-2">
                <div className="flex flex-col">
                  <span className="text-small">{exportType.name}</span>
                  <span className="text-tiny text-default-400 font-semibold">
                    {exportType.extension}
                  </span>
                </div>
              </div>
            </DropdownItem>
          );
        }}
      </DropdownMenu>
    </Dropdown>
  );
}
