import { Command, CommandImplementation, CommandSearchResult, CommandContext } from "./types";
import { useStore } from "@/state/store";
import {
  FolderPlus,
  Download,
  FileText,
  Settings,
  History,
  BarChart3,
  RefreshCw,
  Moon,
  Sun,
  XCircle,
} from "lucide-react";
import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { fuzzyMatch } from "@/lib/fuzzy-matcher";

export class CommandRegistry {
  private commands: Map<string, CommandImplementation> = new Map();
  private fuzzySearchCache: Map<string, CommandSearchResult[]> = new Map();

  registerCommand(command: CommandImplementation): void {
    this.commands.set(command.id, command);
    this.fuzzySearchCache.clear();
  }

  unregisterCommand(id: string): void {
    this.commands.delete(id);
    this.fuzzySearchCache.clear();
  }

  getCommand(id: string): CommandImplementation | undefined {
    return this.commands.get(id);
  }

  getAllCommands(): CommandImplementation[] {
    return Array.from(this.commands.values());
  }

  search(query: string): CommandSearchResult[] {
    if (!query.trim()) {
      return this.getAllCommands()
        .filter((cmd) => this.isCommandEnabled(cmd))
        .map((cmd) => ({ command: cmd, score: 1, matches: [] }));
    }

    const cacheKey = query.toLowerCase();
    if (this.fuzzySearchCache.has(cacheKey)) {
      return this.fuzzySearchCache.get(cacheKey)!;
    }

    const results: CommandSearchResult[] = [];

    for (const command of this.commands.values()) {
      if (!this.isCommandEnabled(command)) continue;

      const searchFields = [
        { text: command.title, weight: 3 },
        { text: command.description || "", weight: 2 },
        { text: command.category || "", weight: 1 },
        ...(command.keywords?.map((kw) => ({ text: kw, weight: 2 })) || []),
      ];

      let bestScore = 0;
      const allMatches: string[] = [];

      for (const field of searchFields) {
        const result = fuzzyMatch(query, field.text);
        if (result) {
          const weightedScore = result.score * field.weight;
          if (weightedScore > bestScore) {
            bestScore = weightedScore;
          }
          // Collect matched characters for highlighting
          for (const match of result.matches) {
            for (let i = match.start; i < match.end; i++) {
              allMatches.push(field.text[i]);
            }
          }
        }
      }

      if (bestScore > 0) {
        results.push({
          command,
          score: bestScore,
          matches: [...new Set(allMatches)],
        });
      }
    }

    results.sort((a, b) => b.score - a.score);
    this.fuzzySearchCache.set(cacheKey, results);
    return results;
  }

  private isCommandEnabled(command: Command): boolean {
    if (command.enabled === undefined) return true;
    if (typeof command.enabled === "boolean") return command.enabled;
    return command.enabled();
  }

  async executeCommand(id: string, context: CommandContext): Promise<void> {
    const command = this.commands.get(id);
    if (!command) {
      throw new Error(`Command ${id} not found`);
    }

    if (!this.isCommandEnabled(command)) {
      throw new Error(`Command ${id} is not enabled`);
    }

    await command.handler(context);
  }
}

export const commandRegistry = new CommandRegistry();

export function registerBuiltinCommands(): void {
  commandRegistry.registerCommand({
    id: "runbook.new",
    title: "New Runbook",
    description: "Create a new runbook in the current workspace",
    category: "Runbook",
    icon: FileText,
    keywords: ["create", "add", "runbook"],
    handler: async () => {
      try {
        await emit("new-runbook");
      } catch (error) {
        console.error("Failed to trigger new runbook:", error);
      }
    },
  });

  commandRegistry.registerCommand({
    id: "workspace.new",
    title: "New Workspace",
    description: "Create a new workspace",
    category: "Workspace",
    icon: FolderPlus,
    keywords: ["create", "add", "workspace", "folder"],
    handler: () => {
      useStore.getState().setNewWorkspaceDialogOpen(true);
    },
  });

  commandRegistry.registerCommand({
    id: "runbook.export",
    title: "Export Runbook",
    description: "Export the current runbook",
    category: "Runbook",
    icon: Download,
    keywords: ["export", "download", "save"],
    enabled: () => {
      // Check if current tab URL starts with /runbook/
      const state = useStore.getState();
      const currentTab = state.tabs.find((tab) => tab.id === state.currentTabId);
      if (!currentTab) return false;

      return currentTab.url.startsWith("/runbook/");
    },
    handler: async () => {
      try {
        await emit("export-markdown");
      } catch (error) {
        console.error("Failed to trigger export:", error);
      }
    },
  });

  commandRegistry.registerCommand({
    id: "app.settings",
    title: "Open Settings",
    description: "Open application settings",
    category: "Application",
    icon: Settings,
    keywords: ["settings", "preferences", "config"],
    handler: () => {
      useStore.getState().openTab("/settings", "Settings");
    },
  });

  commandRegistry.registerCommand({
    id: "app.history",
    title: "Open History",
    description: "Open command history",
    category: "Application",
    icon: History,
    keywords: ["history", "commands", "shell"],
    handler: () => {
      useStore.getState().openTab("/history", "History");
    },
  });

  commandRegistry.registerCommand({
    id: "app.stats",
    title: "Open Statistics",
    description: "View your command statistics",
    category: "Application",
    icon: BarChart3,
    keywords: ["stats", "statistics", "analytics"],
    handler: () => {
      useStore.getState().openTab("/stats", "Statistics");
    },
  });

  commandRegistry.registerCommand({
    id: "app.sync",
    title: "Sync Now",
    description: "Trigger a sync with Atuin server",
    category: "Application",
    icon: RefreshCw,
    keywords: ["sync", "refresh", "update"],
    handler: async () => {
      try {
        await emit("start-sync");
      } catch (error) {
        console.error("Failed to trigger sync:", error);
      }
    },
  });

  commandRegistry.registerCommand({
    id: "app.toggle-dark-mode",
    title: "Toggle Dark Mode",
    description: "Switch between light and dark mode",
    category: "Application",
    icon: () => {
      const colorMode = useStore.getState().colorMode;
      return colorMode === "dark" ? Sun : Moon;
    },
    keywords: ["dark", "light", "theme", "appearance"],
    handler: () => {
      const state = useStore.getState();
      const currentMode = state.colorMode;
      const newMode = currentMode === "dark" ? "light" : "dark";
      state.setColorMode(newMode);
    },
  });

  commandRegistry.registerCommand({
    id: "app.set-system-theme",
    title: "Follow System Theme",
    description: "Automatically match your system's light/dark mode",
    category: "Application",
    icon: RefreshCw,
    keywords: ["system", "auto", "automatic", "theme", "appearance"],
    handler: () => {
      useStore.getState().setColorMode("system");
    },
  });

  commandRegistry.registerCommand({
    id: "runbook.kill-all-terminals",
    title: "Kill All Terminals in Current Runbook",
    description: "Stop all running terminals in the current runbook",
    category: "Runbook",
    icon: XCircle,
    keywords: ["kill", "stop", "terminate", "terminals"],
    enabled: () => {
      const state = useStore.getState();
      const currentTab = state.tabs.find((tab) => tab.id === state.currentTabId);
      if (!currentTab) return false;
      return currentTab.url.startsWith("/runbook/");
    },
    handler: async () => {
      const state = useStore.getState();
      const currentTab = state.tabs.find((tab) => tab.id === state.currentTabId);
      if (!currentTab || !currentTab.url.startsWith("/runbook/")) {
        console.error("Not in a runbook");
        return;
      }
      const runbookId = currentTab.url.split("/").pop();
      if (!runbookId) {
        console.error("Could not get runbook ID");
        return;
      }
      try {
        await invoke("runbook_kill_all_ptys", { runbook: runbookId });
      } catch (error) {
        console.error("Failed to kill terminals:", error);
      }
    },
  });
}
