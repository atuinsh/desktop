import { Command, CommandImplementation, CommandSearchResult, CommandContext } from "./types";
import { useStore } from "@/state/store";
import { FolderPlus, Download, FileText, Settings, History, BarChart3 } from "lucide-react";
import { emit } from "@tauri-apps/api/event";

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

  private fuzzyMatch(query: string, target: string): { score: number; matches: string[] } {
    const targetLower = target.toLowerCase();

    if (targetLower.includes(query.toLowerCase())) {
      return { score: query.length / target.length, matches: [target] };
    }

    let score = 0;
    let queryIndex = 0;
    const matches: string[] = [];
    const queryLower = query.toLowerCase();

    for (let i = 0; i < targetLower.length && queryIndex < queryLower.length; i++) {
      if (targetLower[i] === queryLower[queryIndex]) {
        score += 1 / (i + 1);
        queryIndex++;
        matches.push(target[i]);
      }
    }

    if (queryIndex === queryLower.length) {
      return { score: score / queryLower.length, matches };
    }

    return { score: 0, matches: [] };
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
        command.title,
        command.description || "",
        command.category || "",
        ...(command.keywords || []),
      ];

      let bestScore = 0;
      const allMatches: string[] = [];

      for (const field of searchFields) {
        const { score, matches } = this.fuzzyMatch(query, field);
        if (score > bestScore) {
          bestScore = score;
        }
        if (matches.length > 0) {
          allMatches.push(...matches);
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
}
