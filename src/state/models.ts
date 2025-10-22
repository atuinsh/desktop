import { invoke } from "@tauri-apps/api/core";
import Database from "@tauri-apps/plugin-sql";

export class User {
  username: string;
  email: string;
  avatar_url: string | null;
  bio: string | null;

  constructor(username: string, email: string, bio: string, avatar_url: string) {
    this.username = username;
    this.email = email;
    this.bio = bio;
    this.avatar_url = avatar_url;
  }

  isLoggedIn(): boolean {
    return this.username !== "" && this.username !== null && this.email !== "anon";
  }

  is(otherUser: User): boolean {
    return this.username === otherUser.username && this.email === otherUser.email;
  }
}

export interface UserOrg {
  id: string;
  name: string;
  slug: string;
  avatar_url: string | null;
}

export const DefaultUser: User = new User("Anonymous User", "anon", "An anonymous user", "");

export interface HomeInfo {
  historyCount: number;
  recordCount: number;
  lastSyncTime: Date | null;
  recentCommands: ShellHistory[];
  topCommands: ShellHistory[];
}

export const DefaultHomeInfo: HomeInfo = {
  historyCount: 0,
  recordCount: 0,
  lastSyncTime: new Date(),
  recentCommands: [],
  topCommands: [],
};

export class ShellHistory {
  id: string;
  timestamp: number;
  command: string;
  user: string;
  host: string;
  cwd: string;
  duration: number;
  exit: number;

  /// Pass a row straight from the database to this
  constructor(
    id: string,
    timestamp: number,
    command: string,
    hostuser: string,
    cwd: string,
    duration: number,
    exit: number,
  ) {
    this.id = id;
    this.timestamp = timestamp;
    this.command = command;

    let [host, user] = hostuser.split(":");
    this.user = user;
    this.host = host;

    this.cwd = cwd;
    this.duration = duration;
    this.exit = exit;
  }
}

export interface Alias {
  name: string;
  value: string;
}

export interface Var {
  name: string;
  value: string;
  export: boolean;
}

export interface InspectHistory {
  other: ShellHistory[];
}

// Not yet complete. Not all types are defined here.
// Gonna hold off until the settings refactoring.
export interface Settings {
  auto_sync: boolean;
  update_check: boolean;
  sync_address: string;
  sync_frequency: string;
  db_path: string;
  record_store_path: string;
  key_path: string;
  session_path: string;
  shell_up_key_binding: boolean;
  inline_height: number;
  invert: boolean;
  show_preview: boolean;
  max_preview_height: number;
  show_help: boolean;
  show_tabs: boolean;
  word_chars: string;
  scroll_context_lines: number;
  history_format: string;
  prefers_reduced_motion: boolean;
  store_failed: boolean;
  secrets_filter: boolean;
  workspaces: boolean;
  ctrl_n_shortcuts: boolean;
  network_connect_timeout: number;
  network_timeout: number;
  local_timeout: number;
  enter_accept: boolean;
  smart_sort: boolean;
  sync: Sync;
}

interface Sync {
  records: boolean;
}

export interface RemoteUser {
  id: string;
  avatar_url: string;
  display_name: string | null;
  username: string;
}

export interface RemoteOrg {
  id: string;
  name: string;
  slug: string;
  avatar_url: string | null;
}

interface RemoteSnapshot {
  id: string;
  tag: string;
  inserted_at: string;
  updated_at: string;
}

interface RemoteCollaboration {
  id: string;
  accepted: boolean;
  runbook_id: string;
  user: RemoteUser;
}

type RemoteOwner =
  | {
      type: "user";
      user: RemoteUser;
    }
  | {
      type: "org";
      org: RemoteOrg;
    };

export interface RemoteRunbook {
  id: string;
  name: string;
  slug: string;
  nwo: string;
  visibility: "public" | "private" | "unlisted";
  client_created: string;
  owner: RemoteOwner;
  workspace_id: string;
  snapshots: RemoteSnapshot[];
  collaborations: RemoteCollaboration[];
  permissions: string[];
  content?: any[];
}

// Define other interfaces (Dialect, Timezone, Style, SearchMode, FilterMode, ExitMode, KeymapMode, CursorStyle, WordJumpMode, RegexSet, Stats) accordingly.

export async function inspectCommandHistory(h: ShellHistory): Promise<InspectHistory> {
  const settings: Settings = await invoke("cli_settings");
  const db = await Database.load("sqlite:" + settings.db_path);

  let other: any[] = await db.select(
    "select * from history where command=?1 order by timestamp desc",
    [h.command],
  );
  console.log(other);

  return {
    other: other.map(
      (i) => new ShellHistory(i.id, i.timestamp, i.command, i.hostname, i.cwd, i.duration, i.exit),
    ),
  };
}

export async function inspectDirectoryHistory(h: ShellHistory): Promise<InspectHistory> {
  const settings: Settings = await invoke("cli_settings");
  const db = await Database.load("sqlite:" + settings.db_path);

  let other: any[] = await db.select("select * from history where cwd=?1 order by timestamp desc", [
    h.cwd,
  ]);
  console.log(other);

  return {
    other: other.map(
      (i) => new ShellHistory(i.id, i.timestamp, i.command, i.hostname, i.cwd, i.duration, i.exit),
    ),
  };
}
