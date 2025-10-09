import React, { useState, useEffect, useRef } from "react";
import { ChevronDownIcon } from "lucide-react";

interface DebugWindowProps {
  id: string;
  title: string;
  children: React.ReactNode;
}

interface Position {
  x: number;
  y: number;
}

export default function DebugWindow(props: DebugWindowProps) {
  const [position, setPosition] = useState<Position>({ x: 100, y: 100 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragOffset, setDragOffset] = useState<Position>({ x: 0, y: 0 });
  const [isCollapsed, setIsCollapsed] = useState(false);
  const windowRef = useRef<HTMLDivElement>(null);

  useEffect(
    function loadSavedState() {
      const savedPosition = localStorage.getItem(`debug-window-${props.id}`);
      const savedCollapsed = localStorage.getItem(`debug-window-${props.id}-collapsed`);

      if (savedPosition) {
        try {
          const parsed = JSON.parse(savedPosition) as Position;
          setPosition(parsed);
        } catch (error) {
          console.warn("Failed to parse saved debug window position:", error);
        }
      }

      if (savedCollapsed) {
        try {
          const parsed = JSON.parse(savedCollapsed) as boolean;
          setIsCollapsed(parsed);
        } catch (error) {
          console.warn("Failed to parse saved debug window collapsed state:", error);
        }
      }
    },
    [props.id],
  );

  useEffect(
    function savePosition() {
      localStorage.setItem(`debug-window-${props.id}`, JSON.stringify(position));
    },
    [position, props.id],
  );

  useEffect(
    function saveCollapsedState() {
      localStorage.setItem(`debug-window-${props.id}-collapsed`, JSON.stringify(isCollapsed));
    },
    [isCollapsed, props.id],
  );

  const handleMouseDown = (e: React.MouseEvent) => {
    if (!windowRef.current) return;

    const rect = windowRef.current.getBoundingClientRect();
    setDragOffset({
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    });
    setIsDragging(true);
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (!isDragging) return;

    setPosition({
      x: e.clientX - dragOffset.x,
      y: e.clientY - dragOffset.y,
    });
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  const handleToggleCollapse = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsCollapsed(!isCollapsed);
  };

  useEffect(
    function attachMouseEventListeners() {
      if (isDragging) {
        document.addEventListener("mousemove", handleMouseMove);
        document.addEventListener("mouseup", handleMouseUp);

        return () => {
          document.removeEventListener("mousemove", handleMouseMove);
          document.removeEventListener("mouseup", handleMouseUp);
        };
      }
    },
    [isDragging, dragOffset],
  );

  if (!import.meta.env.DEV) {
    return null;
  }

  return (
    <div
      ref={windowRef}
      className="flex flex-col fixed border bg-background p-2 z-[9999] select-none"
      style={{
        left: `${position.x}px`,
        top: `${position.y}px`,
        cursor: isDragging ? "grabbing" : "default",
      }}
    >
      <div
        className="flex items-center gap-2 text-sm font-bold border-b cursor-move"
        onMouseDown={handleMouseDown}
        style={{ cursor: isDragging ? "grabbing" : "grab" }}
      >
        <button
          onClick={handleToggleCollapse}
          className="flex items-center justify-center w-4 h-4 hover:bg-gray-200 rounded transition-colors"
          style={{ cursor: "pointer" }}
        >
          <ChevronDownIcon
            size={12}
            className={`transition-transform duration-200 ${
              isCollapsed ? "-rotate-90" : "rotate-0"
            }`}
          />
        </button>
        <span className="flex-1">{props.title}</span>
      </div>
      {!isCollapsed && <div>{props.children}</div>}
    </div>
  );
}
