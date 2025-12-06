import { invoke } from "@tauri-apps/api/core";
import { KVStore } from "./kv";
import { supportedShells } from "@/lib/blocks/common/InterpreterSelector";

const PROMETHEUS_URL_KEY = "settings.runbooks.prometheus_url";
const TERMINAL_FONT = "settings.runbooks.terminal.font";
const TERMINAL_FONT_SIZE = "settings.runbooks.terminal.font_size";
const TERMINAL_GL = "settings.runbooks.terminal.gl";
const TERMINAL_GHOSTTY = "settings.runbooks.terminal.ghostty";
const TERMINAL_SHELL = "settings.runbooks.terminal.shell";
const SCRIPT_SHELL = "settings.runbooks.script.shell";
const SCRIPT_INTERPRETERS = "settings.runbooks.script.interpreters";
const EDITOR_VIM_MODE = "settings.editor.vim_mode";
const AI_ENABLED = "settings.ai.enabled";
const AI_API_KEY = "settings.ai.api_key";
const AI_API_ENDPOINT = "settings.ai.api_endpoint";
const AI_MODEL = "settings.ai.model";
const SHELLCHECK_ENABLED = "settings.editor.shellcheck.enabled";
const SHELLCHECK_PATH = "settings.editor.shellcheck.path";

// Notification settings
const NOTIFICATIONS_ENABLED = "settings.notifications.enabled";
const NOTIFICATIONS_VOLUME = "settings.notifications.volume";

// Block notification settings
const NOTIFICATIONS_BLOCK_FINISHED_DURATION = "settings.notifications.block.finished.duration";
const NOTIFICATIONS_BLOCK_FINISHED_SOUND = "settings.notifications.block.finished.sound";
const NOTIFICATIONS_BLOCK_FINISHED_OS = "settings.notifications.block.finished.os";

const NOTIFICATIONS_BLOCK_FAILED_DURATION = "settings.notifications.block.failed.duration";
const NOTIFICATIONS_BLOCK_FAILED_SOUND = "settings.notifications.block.failed.sound";
const NOTIFICATIONS_BLOCK_FAILED_OS = "settings.notifications.block.failed.os";

// Serial execution notification settings
const NOTIFICATIONS_SERIAL_FINISHED_DURATION = "settings.notifications.serial.finished.duration";
const NOTIFICATIONS_SERIAL_FINISHED_SOUND = "settings.notifications.serial.finished.sound";
const NOTIFICATIONS_SERIAL_FINISHED_OS = "settings.notifications.serial.finished.os";

const NOTIFICATIONS_SERIAL_FAILED_DURATION = "settings.notifications.serial.failed.duration";
const NOTIFICATIONS_SERIAL_FAILED_SOUND = "settings.notifications.serial.failed.sound";
const NOTIFICATIONS_SERIAL_FAILED_OS = "settings.notifications.serial.failed.os";

export class Settings {
  public static DEFAULT_FONT = "FiraCode";
  public static DEFAULT_FONT_SIZE = 14;

  public static async runbookPrometheusUrl(val: string | null = null): Promise<string> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(PROMETHEUS_URL_KEY, val);
      return val;
    }

    return (await store.get(PROMETHEUS_URL_KEY)) || "";
  }

  public static async terminalFont(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(TERMINAL_FONT, val);
      return val;
    }

    return await store.get(TERMINAL_FONT);
  }

  public static async terminalFontSize(val: number | null = null): Promise<number | null> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(TERMINAL_FONT_SIZE, val);
      return val;
    }

    return await store.get(TERMINAL_FONT_SIZE);
  }

  public static async terminalGL(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(TERMINAL_GL, val);
      return val;
    }

    return (await store.get(TERMINAL_GL)) || false;
  }

  public static async terminalGhostty(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(TERMINAL_GHOSTTY, val);
      return val;
    }

    return (await store.get(TERMINAL_GHOSTTY)) || false;
  }

  public static async terminalShell(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(TERMINAL_SHELL, val);
      return val;
    }

    return await store.get(TERMINAL_SHELL);
  }

  public static async scriptShell(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(SCRIPT_SHELL, val);
      return val;
    }

    return await store.get(SCRIPT_SHELL);
  }

  public static async scriptInterpreters(): Promise<Array<{ command: string; name: string }>> {
    let store = await KVStore.open_default();
    const interpreters = await store.get<Array<{ command: string; name: string }>>(
      SCRIPT_INTERPRETERS,
    );
    return interpreters || [];
  }

  public static async setScriptInterpreters(
    interpreters: Array<{ command: string; name: string }>,
  ): Promise<void> {
    let store = await KVStore.open_default();
    await store.set(SCRIPT_INTERPRETERS, interpreters);
  }

  public static async editorVimMode(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(EDITOR_VIM_MODE, val);
      return val;
    }

    return (await store.get(EDITOR_VIM_MODE)) || false;
  }

  public static async aiEnabled(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(AI_ENABLED, val);
      return val;
    }

    return (await store.get(AI_ENABLED)) || false;
  }

  public static async aiApiKey(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(AI_API_KEY, val);
      return val;
    }

    return await store.get(AI_API_KEY);
  }

  public static async aiApiEndpoint(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(AI_API_ENDPOINT, val);
      return val;
    }

    return await store.get(AI_API_ENDPOINT);
  }

  public static async aiModel(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(AI_MODEL, val);
      return val;
    }

    return await store.get(AI_MODEL);
  }

  public static async shellCheckEnabled(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(SHELLCHECK_ENABLED, val);
      return val;
    }

    return (await store.get(SHELLCHECK_ENABLED)) || false;
  }

  public static async shellCheckPath(val: string | null = null): Promise<string | null> {
    let store = await KVStore.open_default();

    if (val || val === "") {
      await store.set(SHELLCHECK_PATH, val);
      return val;
    }

    return await store.get(SHELLCHECK_PATH);
  }

  // Notification settings

  public static async notificationsEnabled(val: boolean | null = null): Promise<boolean> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_ENABLED, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_ENABLED)) ?? true;
  }

  public static async notificationsVolume(val: number | null = null): Promise<number> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_VOLUME, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_VOLUME)) ?? 80;
  }

  // Block finished settings
  public static async notificationsBlockFinishedDuration(
    val: number | null = null,
  ): Promise<number> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FINISHED_DURATION, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FINISHED_DURATION)) ?? 5;
  }

  public static async notificationsBlockFinishedSound(val: string | null = null): Promise<string> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FINISHED_SOUND, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FINISHED_SOUND)) ?? "that_was_quick";
  }

  public static async notificationsBlockFinishedOs(
    val: "always" | "not_focused" | "never" | null = null,
  ): Promise<"always" | "not_focused" | "never"> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FINISHED_OS, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FINISHED_OS)) ?? "not_focused";
  }

  // Block failed settings
  public static async notificationsBlockFailedDuration(val: number | null = null): Promise<number> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FAILED_DURATION, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FAILED_DURATION)) ?? 1;
  }

  public static async notificationsBlockFailedSound(val: string | null = null): Promise<string> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FAILED_SOUND, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FAILED_SOUND)) ?? "out_of_nowhere";
  }

  public static async notificationsBlockFailedOs(
    val: "always" | "not_focused" | "never" | null = null,
  ): Promise<"always" | "not_focused" | "never"> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_BLOCK_FAILED_OS, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_BLOCK_FAILED_OS)) ?? "always";
  }

  // Serial finished settings
  public static async notificationsSerialFinishedDuration(
    val: number | null = null,
  ): Promise<number> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FINISHED_DURATION, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FINISHED_DURATION)) ?? 0;
  }

  public static async notificationsSerialFinishedSound(val: string | null = null): Promise<string> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FINISHED_SOUND, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FINISHED_SOUND)) ?? "gracefully";
  }

  public static async notificationsSerialFinishedOs(
    val: "always" | "not_focused" | "never" | null = null,
  ): Promise<"always" | "not_focused" | "never"> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FINISHED_OS, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FINISHED_OS)) ?? "not_focused";
  }

  // Serial failed settings
  public static async notificationsSerialFailedDuration(
    val: number | null = null,
  ): Promise<number> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FAILED_DURATION, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FAILED_DURATION)) ?? 0;
  }

  public static async notificationsSerialFailedSound(val: string | null = null): Promise<string> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FAILED_SOUND, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FAILED_SOUND)) ?? "unexpected";
  }

  public static async notificationsSerialFailedOs(
    val: "always" | "not_focused" | "never" | null = null,
  ): Promise<"always" | "not_focused" | "never"> {
    let store = await KVStore.open_default();

    if (val !== null) {
      await store.set(NOTIFICATIONS_SERIAL_FAILED_OS, val);
      return val;
    }

    return (await store.get(NOTIFICATIONS_SERIAL_FAILED_OS)) ?? "always";
  }

  public static async getSystemDefaultShell(): Promise<string> {
    try {
      const shellPath = await invoke<string>("get_default_shell");

      // Check if this path matches one of our supported shells
      for (const shell of supportedShells) {
        if (shell.paths.includes(shellPath)) {
          return shell.name;
        }
      }

      // If not a known shell, return the full path
      return shellPath;
    } catch (e) {
      console.error("Failed to get system default shell:", e);
      return "bash";
    }
  }

  public static async getEffectiveScriptShell(): Promise<string> {
    const userShell = await this.scriptShell();
    if (userShell) {
      return userShell;
    }
    return await this.getSystemDefaultShell();
  }
}
