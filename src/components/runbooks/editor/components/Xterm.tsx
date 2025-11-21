import { useEffect, useRef, useState, forwardRef, useImperativeHandle } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { Settings } from "@/state/settings.ts";
import useResizeObserver from "use-resize-observer";

/**
 * Calculate terminal row height based on font size.
 * xterm.js with lineHeight: 1 typically renders rows at ~1.2x the font size
 * due to internal padding and line metrics.
 */
export function calculateRowHeight(fontSize: number): number {
  return Math.ceil(fontSize * 1.2);
}

interface XtermProps {
  className?: string;
  height?: number;
  /** Called after terminal renders with actual dimensions for precise sizing */
  onDimensionsReady?: (dimensions: { actualHeight: number; cellHeight: number }) => void;
}

export interface XtermHandle {
  clear: () => void;
  write: (data: string | Uint8Array) => void;
}

const Xterm = forwardRef<XtermHandle, XtermProps>(({ className = "min-h-[200px] w-full", height, onDimensionsReady }, ref) => {
  const [terminal, setTerminal] = useState<Terminal | null>(null);
  const [fitAddon, setFitAddon] = useState<FitAddon | null>(null);
  const [readyToAttach, setReadyToAttach] = useState(false);
  const terminalRef = useRef<HTMLDivElement>(null);
  const writeBuffer = useRef<(string | Uint8Array)[]>([]);
  const clearPending = useRef(false);

  // Expose methods via ref
  useImperativeHandle(
    ref,
    () => ({
      clear: () => {
        if (terminal) {
          terminal.clear();
        } else {
          // Buffer the clear operation
          clearPending.current = true;
          writeBuffer.current = [];
        }
      },
      write: (data: string | Uint8Array) => {
        if (terminal) {
          terminal.write(data);
        } else {
          // Buffer the write operation
          writeBuffer.current.push(data);
        }
      },
    }),
    [terminal],
  );

  // Initialize terminal on mount
  useEffect(() => {
    let fitAddon: FitAddon | null = null;
    let webglAddon: WebglAddon | null = null;

    const initializeTerminal = async () => {
      // Load font settings from Settings (matching pty_state.ts behavior)
      const font = (await Settings.terminalFont()) || Settings.DEFAULT_FONT;
      const fontSize = (await Settings.terminalFontSize()) || Settings.DEFAULT_FONT_SIZE;
      const useWebGL = await Settings.terminalGL();

      const term = new Terminal({
        fontFamily: `${font}, monospace`,
        fontSize: fontSize,
        convertEol: true,
        rescaleOverlappingGlyphs: true,
        letterSpacing: 0,
        lineHeight: 1,
      });

      fitAddon = new FitAddon();
      term.loadAddon(fitAddon);

      // Add WebGL support if enabled in settings
      if (useWebGL) {
        try {
          webglAddon = new WebglAddon();
          term.loadAddon(webglAddon);
        } catch (e) {
          console.warn("WebGL addon failed to load", e);
        }
      }

      setTerminal(term);
      setFitAddon(fitAddon);
    };

    initializeTerminal();

    // Cleanup on unmount
    return () => {
      terminal?.dispose();
      fitAddon?.dispose();
      webglAddon?.dispose();
    };
  }, []);

  // Flush buffered writes when terminal is ready
  useEffect(() => {
    if (!terminal) return;

    // Handle pending clear
    if (clearPending.current) {
      terminal.clear();
      clearPending.current = false;
    }

    // Flush buffered writes
    writeBuffer.current.forEach((data) => terminal.write(data));
    writeBuffer.current = [];
  }, [terminal]);

  // Handle terminal attachment and resizing
  useEffect(() => {
    if (!readyToAttach || !fitAddon || !terminal) return;

    terminal.open(terminalRef.current!);

    // Must call fit() after opening to properly size the terminal to its container
    fitAddon.fit();

    // Report actual dimensions after terminal renders
    // @ts-ignore - accessing internal xterm.js API for accurate cell dimensions
    const cellHeight = terminal._core?._renderService?.dimensions?.css?.cell?.height;
    const actualHeight = terminal.element?.offsetHeight;

    if (actualHeight && cellHeight && onDimensionsReady) {
      onDimensionsReady({ actualHeight, cellHeight });
    }
  }, [terminal, fitAddon, readyToAttach, onDimensionsReady]);

  const compositeRef = (elem: HTMLDivElement) => {
    terminalRef.current = elem;
    resizeRef(elem);
    setReadyToAttach(true);
  };

  const { ref: resizeRef } = useResizeObserver({
    onResize: () => {
      fitAddon?.fit();
    },
  });

  // Explicitly fit when height prop changes and report new dimensions
  useEffect(() => {
    if (!fitAddon || !terminal) return;
    fitAddon.fit();

    // Report updated dimensions after fit
    // @ts-ignore - accessing internal xterm.js API
    const cellHeight = terminal._core?._renderService?.dimensions?.css?.cell?.height;
    const actualHeight = terminal.element?.offsetHeight;

    if (actualHeight && cellHeight && onDimensionsReady) {
      onDimensionsReady({ actualHeight, cellHeight });
    }
  }, [fitAddon, terminal, height, onDimensionsReady]);

  return (
    <div
      ref={compositeRef}
      className={`overflow-hidden ${className}`}
      style={height ? { height: `${height}px` } : undefined}
    />
  );
});

Xterm.displayName = "Xterm";

export default Xterm;
