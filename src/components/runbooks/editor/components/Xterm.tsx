import { useEffect, useRef, useState, forwardRef, useImperativeHandle } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { Settings } from "@/state/settings.ts";
import debounce from "lodash.debounce";

interface XtermProps {
  className?: string;
  visible?: boolean;
}

export interface XtermHandle {
  clear: () => void;
  write: (data: string) => void;
}

const Xterm = forwardRef<XtermHandle, XtermProps>(({ className = "min-h-[200px] w-full", visible = true }, ref) => {
  const [terminal, setTerminal] = useState<Terminal | null>(null);
  const [fitAddon, setFitAddon] = useState<FitAddon | null>(null);
  const terminalRef = useRef<HTMLDivElement>(null);
  const writeBuffer = useRef<string[]>([]);
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
      write: (data: string) => {
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
    const initializeTerminal = async () => {
      const term = new Terminal({
        fontFamily: "FiraCode, monospace",
        fontSize: 14,
        convertEol: true,
      });

      const fit = new FitAddon();
      term.loadAddon(fit);

      // Add WebGL support if enabled in settings
      const useWebGL = await Settings.terminalGL();
      if (useWebGL) {
        try {
          const webglAddon = new WebglAddon();
          term.loadAddon(webglAddon);
        } catch (e) {
          console.warn("WebGL addon failed to load", e);
        }
      }

      setTerminal(term);
      setFitAddon(fit);
    };

    initializeTerminal();

    // Cleanup on unmount
    return () => {
      terminal?.dispose();
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
    if (writeBuffer.current.length > 0) {
      writeBuffer.current.forEach((data) => terminal.write(data));
      writeBuffer.current = [];
    }
  }, [terminal]);

  // Handle terminal attachment and resizing
  useEffect(() => {
    if (!terminal || !terminalRef.current) return;

    terminal.open(terminalRef.current);
    fitAddon?.fit();

    const handleResize = debounce(() => fitAddon?.fit(), 100);
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
    };
  }, [terminal, fitAddon]);

  // Re-fit terminal when visibility changes
  useEffect(() => {
    if (!terminal || !fitAddon || !visible) return;

    // Use requestAnimationFrame to ensure the DOM has updated
    const rafId = requestAnimationFrame(() => {
      fitAddon.fit();
    });

    return () => cancelAnimationFrame(rafId);
  }, [visible, terminal, fitAddon]);

  return <div ref={terminalRef} className={className} style={{ display: visible ? 'block' : 'none' }} />;
});

Xterm.displayName = "Xterm";

export default Xterm;
