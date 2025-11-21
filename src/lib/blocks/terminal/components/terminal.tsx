import { useState, useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { useStore } from "@/state/store";
import useResizeObserver from "use-resize-observer";

const usePersistentTerminal = (pty: string) => {
  const newPtyTerm = useStore((store) => store.newPtyTerm);
  const terminals = useStore((store) => store.terminals);
  const [isReady, setIsReady] = useState(false);

  useEffect(() => {
    (async () => {
      if (!terminals.hasOwnProperty(pty)) {
        // create a new terminal and store it in the store.
        // this means we can resume the same instance even across mount/dismount
        await newPtyTerm(pty);
      }

      setIsReady(true);
    })();

    return () => {
      // We don't dispose of the terminal when the component unmounts
    };
  }, [pty, terminals, newPtyTerm]);

  return { terminalData: terminals[pty], isReady };
};

interface TerminalComponentProps {
  pty: string;
  setCommandRunning: (running: boolean) => void;
  setExitCode: (code: number) => void;
  setCommandDuration: (duration: number | null) => void;
  isFullscreen?: boolean;
  height?: number;
  /** Called after terminal renders with actual dimensions for precise sizing */
  onDimensionsReady?: (dimensions: { actualHeight: number; cellHeight: number }) => void;
  // Additional props used in fullscreen mode (not actively used by component, but passed through)
  block_id?: string;
  script?: string;
  editor?: any;
}

const TerminalComponent = ({
  pty,
  setCommandRunning,
  setExitCode,
  setCommandDuration,
  isFullscreen = false,
  height,
  onDimensionsReady,
}: TerminalComponentProps) => {
  const terminalRef = useRef<HTMLDivElement>(null);
  const { terminalData, isReady } = usePersistentTerminal(pty);
  const [isAttached, setIsAttached] = useState(false);

  // Handle resize via resize observer
  const { ref: resizeRef } = useResizeObserver({
    onResize: () => {
      if (!terminalData || !terminalData.fitAddon) return;
      terminalData.fitAddon.fit();
      const proposedDimensions = terminalData.fitAddon.proposeDimensions();
      if (proposedDimensions) {
        terminalData.terminal.resize(proposedDimensions.cols + 2, proposedDimensions.rows);
      }
    },
  });

  // Combine refs
  const setRefs = (elem: HTMLDivElement | null) => {
    (terminalRef as React.MutableRefObject<HTMLDivElement | null>).current = elem;
    resizeRef(elem);
  };

  useEffect(() => {
    // no pty? no terminal
    if (pty == null) return;

    // the terminal may still be being created so hold off
    if (!isReady) return;

    terminalData.on("command_start", () => {
      setCommandRunning(true);
      setCommandDuration(null);
    });

    terminalData.on("command_end", ({ exitCode, duration }: { exitCode: number; duration: number }) => {
      setExitCode(exitCode);
      setCommandDuration(duration);
      setCommandRunning(false);
    });

    // terminal object needs attaching to a ref to a div
    if (!isAttached && terminalData && terminalData.terminal) {
      // If it's never been attached, attach it
      if (!terminalData.terminal.element && terminalRef.current) {
        terminalData.terminal.open(terminalRef.current);

        // it might have been previously attached, but need moving elsewhere
      } else if (terminalData && terminalRef.current && terminalData.terminal.element) {
        terminalRef.current.appendChild(terminalData.terminal.element);
      }

      // Initial fit
      if (terminalData.fitAddon) {
        terminalData.fitAddon.fit();
      }

      // Report actual dimensions after terminal renders
      // @ts-ignore - accessing internal xterm.js API for accurate cell dimensions
      const cellHeight = terminalData.terminal._core?._renderService?.dimensions?.css?.cell?.height;
      const actualHeight = terminalData.terminal.element?.offsetHeight;
      if (actualHeight && cellHeight && onDimensionsReady) {
        onDimensionsReady({ actualHeight, cellHeight });
      }

      setIsAttached(true);
    }

    // Cleanup: detach terminal element
    return () => {
      if (terminalData && terminalData.terminal && terminalData.terminal.element) {
        if (terminalData.terminal.element.parentElement) {
          terminalData.terminal.element.parentElement.removeChild(terminalData.terminal.element);
        }
        setIsAttached(false);
      }
    };
  }, [terminalData, isReady]);

  // Explicitly fit when height prop changes and report new dimensions
  useEffect(() => {
    if (!terminalData || !terminalData.fitAddon || !isAttached) return;
    terminalData.fitAddon.fit();

    // Report updated dimensions after fit
    // @ts-ignore - accessing internal xterm.js API
    const cellHeight = terminalData.terminal._core?._renderService?.dimensions?.css?.cell?.height;
    const actualHeight = terminalData.terminal.element?.offsetHeight;
    if (actualHeight && cellHeight && onDimensionsReady) {
      onDimensionsReady({ actualHeight, cellHeight });
    }
  }, [terminalData, isAttached, height, onDimensionsReady]);

  if (!isReady) return null;

  // Calculate height: fullscreen uses full height, otherwise use provided height or default
  const containerHeight = isFullscreen ? "100%" : height ? `${height}px` : "400px";

  return (
    <div
      className={`overflow-hidden ${isFullscreen ? "rounded-md" : "rounded-lg"}`}
      style={{
        width: "100%",
        height: containerHeight,
        minWidth: 0,
        display: "block",
        boxSizing: "border-box",
        margin: 0,
        padding: 0,
      }}
      ref={setRefs}
    />
  );
};

export default TerminalComponent;
