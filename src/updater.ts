import Logger from "@/lib/logger";
import { useStore } from "./state/store";
import AtuinEnv from "./atuin_env";
import { Update } from "@tauri-apps/plugin-updater";
import { invoke } from "@tauri-apps/api/core";
const logger = new Logger("Updater");

// These interfaces are largely copied from the tauri-plugin-updater crate,
// as they don't export everything that we need to use.
interface CheckOptions {
  headers?: HeadersInit;
  timeout?: number;
  proxy?: string;
  target?: string;
  allowDowngrades?: boolean;
}

interface UpdateMetadata {
  rid: number;
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  rawJson: Record<string, unknown>;
}

// Custom check function since we need to customize the endpoints.
async function check(options?: CheckOptions): Promise<Update | null> {
  if (options?.headers) {
    options.headers = Array.from(new Headers(options.headers).entries());
  }

  const meta = await invoke<UpdateMetadata | null>("check_for_updates", { ...options });
  return meta ? new Update(meta) : null;
}

export async function checkForAppUpdates(manualActivation: boolean = false): Promise<boolean> {
  logger.debug("Checking for updates...");
  let update = await check();

  if (update?.available) {
    logger.info("Update available!");

    if (update.body) {
      // Fetch everything after the first instance of "Changelog" (with any heading level)
      update.body = /\#{1,6}\s*Changelog\n(.*)$/gms.exec(update.body)?.[1] || update.body;
    } else {
      update.body = "*No changelog available*";
    }

    if (!manualActivation && AtuinEnv.isDev) {
      logger.info("Skipping update prompt in dev for automatic checks");
      return false;
    }

    const state = useStore.getState();
    if (manualActivation && state.showedUpdatePrompt) {
      state.setShowedUpdatePrompt(false);
    }
    state.setAvailableUpdate(update);
  }

  return update?.available ?? false;
}
