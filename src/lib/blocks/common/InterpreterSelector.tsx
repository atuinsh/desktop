import React, { useState, useEffect, useCallback } from "react";
import { Select, SelectItem, SelectSection, Input, Button } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import { PlusIcon, Pencil as PencilIcon, X as XIcon } from "lucide-react";
import { Settings } from "@/state/settings.ts";

// Supported shells with their possible paths
export const supportedShells = [
  { name: "bash", paths: ["/bin/bash"], defaultArgs: "-lc", sshArgs: "-l" },
  { name: "zsh", paths: ["/bin/zsh"], defaultArgs: "-lc", sshArgs: "-l" },
  { name: "fish", paths: ["/usr/bin/fish", "/usr/local/bin/fish", "/opt/homebrew/bin/fish"], defaultArgs: "-c", sshArgs: "" },
  { name: "python3", paths: ["/usr/bin/python3", "/usr/local/bin/python3"], defaultArgs: "-c", sshArgs: "" },
  { name: "node", paths: ["/usr/bin/node", "/usr/local/bin/node"], defaultArgs: "-e", sshArgs: "" },
  { name: "sh", paths: ["/bin/sh"], defaultArgs: "-ic", sshArgs: "-i" },
];

// Helper to build interpreter command string
export const buildInterpreterCommand = (interpreterName: string, isSSH = false) => {
  // Find the shell configuration
  const shellConfig = supportedShells.find(s => s.name === interpreterName);

  if (shellConfig) {
    if (isSSH) {
      // For SSH execution
      if (shellConfig.paths[0].startsWith('/bin/')) {
        // Use absolute path for system shells
        return `${shellConfig.paths[0]} ${shellConfig.sshArgs}`.trim();
      } else {
        // Use env for other shells
        return `/usr/bin/env ${shellConfig.name}${shellConfig.sshArgs ? ' ' + shellConfig.sshArgs : ''}`;
      }
    } else {
      // For local execution
      if (shellConfig.paths[0].startsWith('/bin/')) {
        // Use absolute path for system shells
        return `${shellConfig.paths[0]} ${shellConfig.defaultArgs}`.trim();
      } else {
        // Use env for other shells
        return `/usr/bin/env ${shellConfig.name}${shellConfig.defaultArgs ? ' ' + shellConfig.defaultArgs : ''}`;
      }
    }
  }

  // Fallback for unknown shells
  return `/usr/bin/env ${interpreterName}`;
};

interface InterpreterSelectorProps {
  interpreter: string;
  onInterpreterChange: (interpreter: string) => void;
  size?: "sm" | "md" | "lg";
  variant?: "flat" | "bordered" | "underlined" | "faded";
  isSSH?: boolean;
}

const InterpreterSelector: React.FC<InterpreterSelectorProps> = ({
  interpreter,
  onInterpreterChange,
  size = "sm",
  variant = "flat",
}) => {
  // Track available shells and custom interpreters
  const [availableShells, setAvailableShells] = useState<Record<string, boolean>>({});
  const [isShellMissing, setIsShellMissing] = useState(false);
  const [isCustom, setIsCustom] = useState(!supportedShells.some(shell => shell.name === interpreter));
  const [customValue, setCustomValue] = useState(isCustom ? interpreter : "");
  const [showCustomInput, setShowCustomInput] = useState(false);
  const [scriptInterpreters, setScriptInterpreters] = useState<Array<{command: string; name: string}>>([]);
  const [systemDefaultShell, setSystemDefaultShell] = useState<string>("bash");

  useEffect(() => {
    // Update isCustom state once scriptInterpreters are loaded
    const isInterpreterSupported = supportedShells.some(shell => shell.name === interpreter) || 
      scriptInterpreters.some(shell => shell.name === interpreter);
    
    setIsCustom(!isInterpreterSupported);
  }, [interpreter, scriptInterpreters, supportedShells]);

  // Function to load script interpreters from settings
  const loadScriptInterpreters = useCallback(async () => {
    try {
      const interpreters = await Settings.scriptInterpreters();
      setScriptInterpreters(interpreters);
    } catch (error) {
      console.error("Failed to load script interpreters:", error);
    }
  }, []);

  // Load script interpreters and system default shell on mount
  useEffect(() => {
    loadScriptInterpreters();
    Settings.getSystemDefaultShell().then(setSystemDefaultShell);
  }, [loadScriptInterpreters]);

  // Check which shells are installed
  useEffect(() => {
    const checkShellsAvailable = async () => {
      try {
        const shellStatus: Record<string, boolean> = {};

        // Check each supported shell
        for (const shell of supportedShells) {
          // Skip bash and sh as they're always available
          if (shell.name === "bash" || shell.name === "sh") {
            shellStatus[shell.name] = true;
            continue;
          }

          // Check each possible path for this shell
          let found = false;
          for (const path of shell.paths) {
            try {
              const exists = await invoke<boolean>("check_binary_exists", { path });
              if (exists) {
                found = true;
                break;
              }
            } catch (e) {
              console.error(`Error checking ${path}:`, e);
            }
          }

          shellStatus[shell.name] = found;
        }

        setAvailableShells(shellStatus);

        // Check if currently selected shell is available
        if (interpreter !== "bash" && interpreter !== "sh") {
          setIsShellMissing(interpreter in shellStatus && !shellStatus[interpreter]);
        } else {
          setIsShellMissing(false);
        }
      } catch (error) {
        console.error("Failed to check available shells:", error);
      }
    };

    checkShellsAvailable();
  }, [interpreter]);

  // Function to add a custom interpreter
  const handleAddCustom = useCallback(() => {
    if (customValue.trim() === '') return;
    setIsCustom(true);
    onInterpreterChange(customValue);
    setShowCustomInput(false);
  }, [customValue, onInterpreterChange]);
  
  // Handle editing current custom value
  const handleEditCustom = useCallback(() => {
    setCustomValue(interpreter);
    setShowCustomInput(true);
  }, [interpreter]);

  return (
    <>
      {showCustomInput ? (
        <div className="inline-flex items-center gap-2 min-w-[300px]">
          <Input
            size="sm"
            variant="flat"
            placeholder="/bin/bash -c"
            value={customValue}
            onChange={(e) => setCustomValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleAddCustom()}
            autoFocus
          />
          <Button size="sm" onPress={handleAddCustom} isIconOnly><PlusIcon className="h-3 w-3" /></Button>
          <Button size="sm" variant="light" onPress={() => setShowCustomInput(false)} isIconOnly><XIcon className="h-3 w-3" /></Button>
        </div>
      ) : (
        <div className="flex items-center gap-1 min-w-[300px]">
          <Select
            disableAnimation
            size={size}
            variant={variant}
            selectionMode="single"
            popoverProps={{
              classNames: {
                content: "min-w-[300px]"
              }
            }}
            classNames={{
              trigger: "min-h-unit-8 min-w-[180px]",
              base: "min-w-[180px]"
            }}
            selectedKeys={[interpreter]}
            onOpenChange={(isOpen) => {
              if (isOpen) {
                // Refresh interpreters when dropdown opens
                loadScriptInterpreters();
              }
            }}
            onSelectionChange={(e) => {
              if (!e.currentKey) return;
              const key = e.currentKey as string;
              if (key === "_custom") {
                setShowCustomInput(true);
                return;
              }
              if (key === "_system_default") {
                setIsCustom(false);
                onInterpreterChange(systemDefaultShell);
                return;
              }
              setIsCustom(false);
              onInterpreterChange(key);
            }}
            aria-label="Select interpreter"
          >
            {/* Add custom interpreter as first option if we're using one */}
            {isCustom ? (
              <SelectItem 
                key={interpreter} 
                classNames={{ base: "py-1", description: "text-xs opacity-70" }}
                description="Custom interpreter"
              >
                {interpreter}
              </SelectItem>
            ) : null}

            <SelectSection title="System">
              <SelectItem
                key="_system_default"
                classNames={{ base: "py-1", description: "text-xs opacity-70" }}
                description={`Use system default (${systemDefaultShell})`}
              >
                System Default
              </SelectItem>
            </SelectSection>
            <SelectSection title="Shells">
              {supportedShells.map(shell => {
                // Always show bash and sh, or any shell that's available, or the current selected shell
                const shouldShow = shell.name === "bash" ||
                  shell.name === "sh" ||
                  availableShells[shell.name] ||
                  interpreter === shell.name;

                return shouldShow ? (
                  <SelectItem 
                    key={shell.name} 
                    aria-label={shell.name}
                    className={interpreter === shell.name && isShellMissing ? "text-red-500" : ""}
                    classNames={{ base: "py-1", description: "text-xs opacity-70" }}
                    description={buildInterpreterCommand(shell.name).replace("/usr/bin/env ", "")}
                  >
                    {shell.name}
                  </SelectItem>
                ) : null;
              })}
            </SelectSection>

            {scriptInterpreters.length > 0 ? (
              <SelectSection title="Saved">
                {scriptInterpreters.map(interpreter => (
                  <SelectItem 
                    key={interpreter.command}
                    classNames={{ base: "py-1", description: "text-xs opacity-70" }}
                    description={interpreter.command}
                  >
                    {interpreter.name}
                  </SelectItem>
                ))}
              </SelectSection>
            ) : null}

            <SelectSection title="Actions">
              <SelectItem
                key="_custom"
                startContent={<PlusIcon className="h-3 w-3" />}
                classNames={{ description: "text-xs opacity-70" }}
                description="Add custom"
              >
                Custom
              </SelectItem>
            </SelectSection>
          </Select>
          
          {isCustom && (
            <Button 
              size="sm" 
              isIconOnly 
              variant="light" 
              className="text-gray-500" 
              onPress={handleEditCustom}
            >
              <PencilIcon className="h-3 w-3" />
            </Button>
          )}
        </div>
      )}
    </>
  );
};

export default InterpreterSelector;