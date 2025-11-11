import React from "react";
import { Button, Tooltip } from "@heroui/react";
import { PanelRightCloseIcon } from "lucide-react";
import AgentChat from "./AgentChat";

interface RightSidebarProps {
  onCollapse: () => void;
}

const RightSidebar = React.memo(function RightSidebar({ onCollapse }: RightSidebarProps) {
  return (
    <div className="flex flex-col h-full border-l relative">
      <div className="flex items-center justify-between gap-2 p-2 border-b">
        <Tooltip content="Collapse sidebar" placement="left" delay={300} closeDelay={0}>
          <Button
            isIconOnly
            onPress={onCollapse}
            size="sm"
            variant="light"
          >
            <PanelRightCloseIcon size={18} className="stroke-gray-500" />
          </Button>
        </Tooltip>
      </div>
      <div className="flex-1 overflow-hidden">
        <AgentChat />
      </div>
    </div>
  );
});

export default RightSidebar;

