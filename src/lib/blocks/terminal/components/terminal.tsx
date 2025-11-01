import { useState, useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { useStore } from "@/state/store";

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

const TerminalComponent = ({
  pty,
  setCommandRunning,
  setExitCode,
  setCommandDuration,
  isFullscreen = false,
}: any) => {
  const terminalRef = useRef(null);
  const { terminalData, isReady } = usePersistentTerminal(pty);
  const [isAttached, setIsAttached] = useState(false);

  useEffect(() => {
    // no pty? no terminal
    if (pty == null) return;

    // the terminal may still be being created so hold off
    if (!isReady) return;

    terminalData.on("command_start", () => {
      setCommandRunning(true);
      setCommandDuration(null);
    });

    terminalData.on("command_end", ({ exitCode, duration }) => {
      setExitCode(exitCode);
      setCommandDuration(duration);
      setCommandRunning(false);
    });

    const windowResize = () => {
      if (!terminalData || !terminalData.fitAddon) return;

      // Add a small delay to ensure the DOM has updated
      setTimeout(() => {
        terminalData.fitAddon.fit();
        let proposedDimensions = terminalData.fitAddon.proposeDimensions();
        if (proposedDimensions) {
          terminalData.terminal.resize(proposedDimensions.cols + 2, proposedDimensions.rows);
        }
      }, 100);
    };

    // terminal object needs attaching to a ref to a div
    if (!isAttached && terminalData && terminalData.terminal) {
      // If it's never been attached, attach it
      if (!terminalData.terminal.element && terminalRef.current) {
        terminalData.terminal.open(terminalRef.current);

        // it might have been previously attached, but need moving elsewhere
      } else if (terminalData && terminalRef.current) {
        // @ts-ignore
        terminalRef.current.appendChild(terminalData.terminal.element);
      }

      windowResize();
      setIsAttached(true);

      window.addEventListener("resize", windowResize);
    }

    // Customize further as needed
    return () => {
      if (terminalData && terminalData.terminal && terminalData.terminal.element) {
        // Instead of removing, we just detach
        if (terminalData.terminal.element.parentElement) {
          terminalData.terminal.element.parentElement.removeChild(terminalData.terminal.element);
        }
        setIsAttached(false);
      }

      window.removeEventListener("resize", windowResize);
    };
  }, [terminalData, isReady]);

  if (!isReady) return null;

  return (
    <div
      className={`overflow-hidden ${isFullscreen ? "h-full rounded-md" : "rounded-lg h-[400px]"}`}
      style={{
        width: "100%",
        minWidth: 0,
        display: "block",
        boxSizing: "border-box",
        margin: 0,
        padding: 0,
      }}
      ref={terminalRef}
    />
  );
};

export default TerminalComponent;
