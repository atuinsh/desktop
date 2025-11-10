import React from "react";
import { Button, Tooltip } from "@heroui/react";
import { PanelRightCloseIcon } from "lucide-react";

interface RightSidebarProps {
  onCollapse: () => void;
}

const RightSidebar = React.memo(function RightSidebar({ onCollapse }: RightSidebarProps) {
  return (
    <div className="flex flex-col h-full overflow-y-auto border-l relative">
      <div className="flex items-center gap-2 p-3">
        <Tooltip content="Collapse" placement="right" delay={300} closeDelay={0}>
          <Button
            isIconOnly
            onPress={onCollapse}
            size="sm"
            variant="light"
          >
            <PanelRightCloseIcon size={20} className="stroke-gray-500" />
          </Button>
        </Tooltip>
        <h2 className="text-lg font-semibold">Right Sidebar</h2>
      </div>
      <div className="p-4">
        <div className="text-sm text-gray-500">Content coming soon...</div>
      </div>
    </div>
  );
});

export default RightSidebar;

