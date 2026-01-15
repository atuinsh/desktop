import { useCallback } from "react";

export interface UseResizableOptions {
  /** Current width value */
  width: number;
  /** Callback to update width */
  onWidthChange: (width: number) => void;
  /** Minimum allowed width */
  minWidth: number;
  /** Maximum allowed width */
  maxWidth: number;
  /**
   * Which edge the resize handle is on.
   * - "right": Handle on right edge, drag right = wider (default)
   * - "left": Handle on left edge, drag left = wider
   */
  edge?: "left" | "right";
  /** Whether to dispatch a window resize event on completion (for xterm reflow, etc.) */
  dispatchResizeEvent?: boolean;
}

export interface UseResizableReturn {
  /** Mouse down handler for the resize handle */
  onResizeStart: (e: React.MouseEvent) => void;
}

/**
 * Hook for creating resizable panels with mouse drag.
 *
 * @example
 * ```tsx
 * const { onResizeStart } = useResizable({
 *   width: sidebarWidth,
 *   onWidthChange: setSidebarWidth,
 *   minWidth: 200,
 *   maxWidth: 500,
 *   edge: "right",
 * });
 *
 * return (
 *   <div style={{ width }}>
 *     <div onMouseDown={onResizeStart} className="resize-handle" />
 *   </div>
 * );
 * ```
 */
export function useResizable({
  width,
  onWidthChange,
  minWidth,
  maxWidth,
  edge = "right",
  dispatchResizeEvent = false,
}: UseResizableOptions): UseResizableReturn {
  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startWidth = width;

      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const handleMouseMove = (e: MouseEvent) => {
        const rawDelta = e.clientX - startX;
        // For left edge, dragging left (negative delta) should increase width
        const delta = edge === "left" ? -rawDelta : rawDelta;
        const newWidth = Math.min(Math.max(startWidth + delta, minWidth), maxWidth);
        onWidthChange(newWidth);
      };

      const handleMouseUp = () => {
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        document.removeEventListener("mousemove", handleMouseMove);
        document.removeEventListener("mouseup", handleMouseUp);

        if (dispatchResizeEvent) {
          window.dispatchEvent(new Event("resize"));
        }
      };

      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
    },
    [width, onWidthChange, minWidth, maxWidth, edge, dispatchResizeEvent]
  );

  return { onResizeStart };
}
