import { useEffect, useRef, useState, forwardRef, useImperativeHandle } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { Settings } from "@/state/settings.ts";
import useResizeObserver from "use-resize-observer";

interface XtermProps {
  className?: string;
}

export interface XtermHandle {
  clear: () => void;
  write: (data: string | Uint8Array) => void;
}

const Xterm = forwardRef<XtermHandle, XtermProps>(({ className = "min-h-[200px] w-full" }, ref) => {
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
      const term = new Terminal({
        fontFamily: "FiraCode, monospace",
        fontSize: 14,
        convertEol: true,
      });

      fitAddon = new FitAddon();
      term.loadAddon(fitAddon);

      // Add WebGL support if enabled in settings
      const useWebGL = await Settings.terminalGL();
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
  }, [terminal, fitAddon, readyToAttach]);

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

  return <div ref={compositeRef} className={className} />;
});

Xterm.displayName = "Xterm";

export default Xterm;
