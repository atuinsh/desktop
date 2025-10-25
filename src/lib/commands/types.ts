import { LucideIcon } from "lucide-react";

export interface Command {
  id: string;
  title: string;
  description?: string;
  category?: string;
  icon?: LucideIcon | (() => LucideIcon);
  keywords?: string[];
  shortcut?: string[];
  enabled?: boolean | (() => boolean);
}

export interface CommandContext {
  currentWorkspaceId?: string;
  currentRunbookId?: string;
  [key: string]: any;
}

export type CommandHandler = (context: CommandContext) => void | Promise<void>;

export interface CommandImplementation extends Command {
  handler: CommandHandler;
}

export interface CommandSearchResult {
  command: Command;
  score: number;
  matches: string[];
}
