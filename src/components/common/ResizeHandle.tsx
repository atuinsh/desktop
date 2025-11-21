import { useCallback, useEffect, useRef, useState } from "react";
import { GripHorizontal } from "lucide-react";

interface ResizeHandleProps {
  /** Current height in pixels */
  currentHeight: number;
  /** Callback when resize completes with new height (persists to block props) */
  onResize: (newHeight: number) => void;
  /** Optional: callback during drag for real-time preview */
  onResizePreview?: (newHeight: number) => void;
  /** Optional: snap to this increment (e.g., row height) */
  snapIncrement?: number;
  /** Minimum height in pixels */
  minHeight?: number;
  /** Maximum height in pixels */
  maxHeight?: number;
  /** Optional className for the handle */
  className?: string;
}

/**
 * A draggable resize handle for vertical resizing.
 * Supports snapping to increments (like terminal row heights).
 */
export default function ResizeHandle({
  currentHeight,
  onResize,
  onResizePreview,
  snapIncrement,
  minHeight = 100,
  maxHeight = 800,
  className = "",
}: ResizeHandleProps) {
  const [isDragging, setIsDragging] = useState(false);
  const startY = useRef(0);
  const startHeight = useRef(0);
  const lastPreviewHeight = useRef<number | null>(null);

  const snapToIncrement = useCallback(
    (height: number): number => {
      if (!snapIncrement) return height;
      return Math.round(height / snapIncrement) * snapIncrement;
    },
    [snapIncrement],
  );

  const clampHeight = useCallback(
    (height: number): number => {
      return Math.max(minHeight, Math.min(maxHeight, height));
    },
    [minHeight, maxHeight],
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(true);
      startY.current = e.clientY;
      startHeight.current = currentHeight;
      lastPreviewHeight.current = null;
    },
    [currentHeight],
  );

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaY = e.clientY - startY.current;
      const newHeight = startHeight.current + deltaY;
      const clampedHeight = clampHeight(newHeight);
      const snappedHeight = snapToIncrement(clampedHeight);

      // Only call preview if height actually changed
      if (snappedHeight !== lastPreviewHeight.current) {
        lastPreviewHeight.current = snappedHeight;
        onResizePreview?.(snappedHeight);
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      if (lastPreviewHeight.current !== null) {
        onResize(lastPreviewHeight.current);
      }
      lastPreviewHeight.current = null;
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, onResize, onResizePreview, clampHeight, snapToIncrement]);

  return (
    <div
      className={`
        flex items-center justify-center
        h-2 cursor-ns-resize select-none
        hover:bg-default-200 active:bg-default-300
        transition-colors duration-150
        group
        ${isDragging ? "bg-default-300" : ""}
        ${className}
      `}
      onMouseDown={handleMouseDown}
    >
      <GripHorizontal
        size={14}
        className={`
          text-default-400
          group-hover:text-default-600
          ${isDragging ? "text-default-600" : ""}
        `}
      />
    </div>
  );
}
