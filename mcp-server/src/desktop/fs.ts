import { readdir, readFile, writeFile, mkdir, unlink, stat } from "node:fs/promises";
import { join, extname } from "node:path";
import { homedir } from "node:os";
import { Database } from "bun:sqlite";
import { parse as parseYAML, stringify as stringifyYAML } from "yaml";
import type { RunbookFile, WorkspaceConfig, WorkspaceInfo, RunbookSummary } from "../types.js";

const RUNBOOK_EXT = ".atrb";

// ── Database paths ─────────────────────────────────────────────────

const APP_SUPPORT_DIR = join(
  homedir(),
  "Library",
  "Application Support",
  "sh.atuin.app"
);

function getDbPath(): string {
  // Check if a custom path is set via env
  if (process.env.ATUIN_MCP_DB_PATH) {
    return process.env.ATUIN_MCP_DB_PATH;
  }
  // Default: production DB. Set ATUIN_MCP_DEV=1 to use dev DB.
  const prefix = process.env.ATUIN_MCP_DEV ? "dev_" : "";
  return join(APP_SUPPORT_DIR, `${prefix}runbooks.db`);
}

function openDb(readonly = false): Database {
  return new Database(getDbPath(), { readwrite: !readonly, create: false });
}

// ── Default workspace root ─────────────────────────────────────────

export function getDefaultWorkspaceRoot(): string {
  return join(homedir(), "Documents", "Atuin Runbooks");
}

// ── TOML helpers ───────────────────────────────────────────────────

function parseWorkspaceToml(content: string): WorkspaceConfig {
  const lines = content.split("\n");
  let id = "";
  let name = "";
  let inWorkspace = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "[workspace]") {
      inWorkspace = true;
      continue;
    }
    if (trimmed.startsWith("[") && trimmed !== "[workspace]") {
      inWorkspace = false;
      continue;
    }
    if (!inWorkspace) continue;

    const match = trimmed.match(/^(\w+)\s*=\s*"(.+)"$/);
    if (match) {
      if (match[1] === "id") id = match[2];
      if (match[1] === "name") name = match[2];
    }
  }

  return { workspace: { id, name } };
}

function generateWorkspaceToml(config: WorkspaceConfig): string {
  return `[workspace]\nid = "${config.workspace.id}"\nname = "${config.workspace.name}"\n`;
}

// ── ID generation ──────────────────────────────────────────────────

export function generateId(): string {
  return crypto.randomUUID();
}

// ── Filename helpers ───────────────────────────────────────────────

function sanitizeFilename(name: string): string {
  return name.replace(/[^a-zA-Z0-9 _-]/g, "").trim();
}

// ── Workspace operations (DB + filesystem) ─────────────────────────

interface DbWorkspaceRow {
  id: string;
  name: string;
  online: number;
  folder: string | null;
  org_id: string | null;
}

/**
 * List all workspaces from the desktop app's SQLite database,
 * enriched with runbook info from their folders on disk.
 */
export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  const workspaces: WorkspaceInfo[] = [];

  let rows: DbWorkspaceRow[];
  try {
    const db = openDb(true);
    rows = db.query("SELECT id, name, online, folder, org_id FROM workspaces").all() as DbWorkspaceRow[];
    db.close();
  } catch {
    // DB not available — fall back to filesystem scan
    return listWorkspacesFromDisk();
  }

  for (const row of rows) {
    // Only show offline (local) workspaces that have a folder
    if (!row.folder) continue;

    const runbooks = await listRunbooksInDir(row.folder).catch(() => []);
    workspaces.push({
      id: row.id,
      name: row.name ?? "Unnamed",
      path: row.folder,
      runbooks,
    });
  }

  return workspaces;
}

/**
 * Fallback: scan filesystem for workspaces (when DB is unavailable).
 */
async function listWorkspacesFromDisk(): Promise<WorkspaceInfo[]> {
  const root = getDefaultWorkspaceRoot();
  const workspaces: WorkspaceInfo[] = [];

  let entries: string[];
  try {
    entries = await readdir(root);
  } catch {
    return [];
  }

  for (const entry of entries) {
    const dirPath = join(root, entry);
    const dirStat = await stat(dirPath).catch(() => null);
    if (!dirStat?.isDirectory()) continue;

    const tomlPath = join(dirPath, "atuin.toml");
    let config: WorkspaceConfig;
    try {
      const tomlContent = await readFile(tomlPath, "utf-8");
      config = parseWorkspaceToml(tomlContent);
    } catch {
      continue;
    }

    const runbooks = await listRunbooksInDir(dirPath);
    workspaces.push({
      id: config.workspace.id,
      name: config.workspace.name,
      path: dirPath,
      runbooks,
    });
  }

  return workspaces;
}

/**
 * List runbook files in a directory (non-recursive).
 */
async function listRunbooksInDir(dirPath: string): Promise<RunbookSummary[]> {
  const entries = await readdir(dirPath);
  const runbooks: RunbookSummary[] = [];

  for (const entry of entries) {
    if (extname(entry) !== RUNBOOK_EXT) continue;
    const filePath = join(dirPath, entry);
    try {
      const content = await readFile(filePath, "utf-8");
      const parsed = parseYAML(content) as RunbookFile;
      runbooks.push({
        id: parsed.id,
        name: parsed.name,
        path: filePath,
        blockCount: parsed.content?.length ?? 0,
      });
    } catch {
      // Skip corrupt files
    }
  }

  return runbooks;
}

/**
 * Read a runbook by file path.
 */
export async function readRunbook(filePath: string): Promise<RunbookFile> {
  const content = await readFile(filePath, "utf-8");
  return parseYAML(content) as RunbookFile;
}

/**
 * Find a runbook by name or ID across all workspaces.
 */
export async function findRunbook(
  nameOrId: string
): Promise<{ runbook: RunbookFile; path: string; workspace: WorkspaceInfo } | null> {
  const workspaces = await listWorkspaces();
  for (const ws of workspaces) {
    for (const rb of ws.runbooks) {
      if (rb.id === nameOrId || rb.name.toLowerCase() === nameOrId.toLowerCase()) {
        const runbook = await readRunbook(rb.path);
        return { runbook, path: rb.path, workspace: ws };
      }
    }
  }
  return null;
}

/**
 * Write a runbook to disk as a .atrb YAML file.
 */
export async function writeRunbook(
  workspacePath: string,
  runbook: RunbookFile
): Promise<string> {
  const filename = sanitizeFilename(runbook.name) + RUNBOOK_EXT;
  const filePath = join(workspacePath, filename);
  const yamlContent = stringifyYAML(runbook, { lineWidth: 0 });
  await writeFile(filePath, yamlContent, "utf-8");
  return filePath;
}

/**
 * Delete a runbook file.
 */
export async function deleteRunbook(filePath: string): Promise<void> {
  await unlink(filePath);
}

/**
 * Create a new workspace: directory + atuin.toml + register in desktop app's DB.
 */
export async function createWorkspace(name: string): Promise<WorkspaceInfo> {
  const root = getDefaultWorkspaceRoot();
  const dirName = sanitizeFilename(name);
  const dirPath = join(root, dirName);

  await mkdir(dirPath, { recursive: true });

  const id = generateId();
  const config: WorkspaceConfig = {
    workspace: { id, name },
  };

  await writeFile(join(dirPath, "atuin.toml"), generateWorkspaceToml(config), "utf-8");

  // Register in the desktop app's SQLite database
  try {
    const now = Date.now();
    const db = openDb();
    db.run(
      `INSERT OR IGNORE INTO workspaces (id, name, online, folder, created, updated)
       VALUES (?, ?, 0, ?, ?, ?)`,
      [id, name, dirPath, now, now]
    );
    db.close();
  } catch (e) {
    // DB registration failed — workspace still works on disk, just won't show in app
    console.error(`Warning: could not register workspace in desktop DB: ${e}`);
  }

  return {
    id,
    name,
    path: dirPath,
    runbooks: [],
  };
}
