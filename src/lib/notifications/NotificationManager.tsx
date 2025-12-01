import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import {
  grandCentral,
  GrandCentralEvents,
  onSerialExecutionStarted,
  onSerialExecutionCompleted,
  onSerialExecutionFailed,
} from "@/lib/events/grand_central";
import { Settings } from "@/state/settings";
import Runbook from "@/state/runbooks/runbook";
import Logger from "@/lib/logger";

const logger = new Logger("Notifications", "#6b7280", "#9ca3af");
(window as any).sendNotification = sendNotification;

interface BlockExecution {
  blockId: string;
  runbookId: string;
  startTime: number;
}

interface SerialExecution {
  runbookId: string;
  startTime: number;
}

type SoundOption = string;
type OsOption = "always" | "not_focused" | "never";

interface EventNotificationConfig {
  duration: number;
  sound: SoundOption;
  os: OsOption;
}

interface NotificationSettings {
  enabled: boolean;
  volume: number;
  blockFinished: EventNotificationConfig;
  blockFailed: EventNotificationConfig;
  serialFinished: EventNotificationConfig;
  serialFailed: EventNotificationConfig;
}

type NotificationPayload = {
  title: string;
  body: string;
  success: boolean;
  duration: number;
};

async function loadSettings(): Promise<NotificationSettings> {
  const [
    enabled,
    volume,
    blockFinishedDuration,
    blockFinishedSound,
    blockFinishedOs,
    blockFailedDuration,
    blockFailedSound,
    blockFailedOs,
    serialFinishedDuration,
    serialFinishedSound,
    serialFinishedOs,
    serialFailedDuration,
    serialFailedSound,
    serialFailedOs,
  ] = await Promise.all([
    Settings.notificationsEnabled(),
    Settings.notificationsVolume(),
    Settings.notificationsBlockFinishedDuration(),
    Settings.notificationsBlockFinishedSound(),
    Settings.notificationsBlockFinishedOs(),
    Settings.notificationsBlockFailedDuration(),
    Settings.notificationsBlockFailedSound(),
    Settings.notificationsBlockFailedOs(),
    Settings.notificationsSerialFinishedDuration(),
    Settings.notificationsSerialFinishedSound(),
    Settings.notificationsSerialFinishedOs(),
    Settings.notificationsSerialFailedDuration(),
    Settings.notificationsSerialFailedSound(),
    Settings.notificationsSerialFailedOs(),
  ]);

  return {
    enabled,
    volume,
    blockFinished: {
      duration: blockFinishedDuration,
      sound: blockFinishedSound,
      os: blockFinishedOs,
    },
    blockFailed: { duration: blockFailedDuration, sound: blockFailedSound, os: blockFailedOs },
    serialFinished: {
      duration: serialFinishedDuration,
      sound: serialFinishedSound,
      os: serialFinishedOs,
    },
    serialFailed: { duration: serialFailedDuration, sound: serialFailedSound, os: serialFailedOs },
  };
}

function playNotificationSound(soundId: string, volume: number) {
  if (soundId === "none") return;

  invoke("play_sound", { soundId, volume: volume / 100 }).catch((e) => {
    logger.warn("Failed to play notification sound:", e);
  });
}

async function sendOsNotification(payload: NotificationPayload): Promise<void> {
  // Try Web Notification API first (works better in dev mode)
  if ("Notification" in window) {
    try {
      if (Notification.permission === "granted") {
        logger.debug("Sending Web notification:", payload.title);
        new Notification(payload.title, { body: payload.body });
        logger.debug("Web notification sent successfully");
        return;
      } else if (Notification.permission !== "denied") {
        logger.debug("Requesting Web notification permission...");
        const permission = await Notification.requestPermission();
        if (permission === "granted") {
          new Notification(payload.title, { body: payload.body });
          logger.debug("Web notification sent successfully");
          return;
        }
        logger.debug("Web notification permission result:", permission);
      }
    } catch (e) {
      logger.warn("Web notification failed, trying Tauri:", e);
    }
  }

  // Fallback to Tauri notification
  try {
    let hasPermission = await isPermissionGranted();
    logger.debug("Tauri notification permission status:", hasPermission);

    if (!hasPermission) {
      logger.debug("Requesting Tauri notification permission...");
      const permission = await requestPermission();
      logger.debug("Tauri permission response:", permission);
      hasPermission = permission === "granted";
    }

    if (hasPermission) {
      logger.debug("Sending Tauri notification:", payload.title);
      await sendNotification({
        title: payload.title,
        body: payload.body,
      });
      logger.debug("Tauri notification sent successfully");
    } else {
      logger.warn("No notification permission available");
    }
  } catch (e) {
    logger.error("Tauri notification failed:", e);
  }
}

function shouldSendOsNotification(config: EventNotificationConfig): boolean {
  switch (config.os) {
    case "always":
      return true;
    case "not_focused":
      return !document.hasFocus();
    case "never":
      return false;
  }
}

export default function NotificationManager() {
  const executionsRef = useRef<Map<string, BlockExecution>>(new Map());
  const serialExecutionsRef = useRef<Map<string, SerialExecution>>(new Map());
  const settingsRef = useRef<NotificationSettings | null>(null);

  const refreshSettings = useCallback(async () => {
    settingsRef.current = await loadSettings();
  }, []);

  const shouldNotify = useCallback(
    (durationSecs: number, config: EventNotificationConfig): boolean => {
      const settings = settingsRef.current;
      if (!settings || !settings.enabled) return false;

      // Always require at least 0.5s to filter out instant executions (e.g., dropdown mount)
      const MIN_DURATION = 0.5;
      const effectiveThreshold = Math.max(config.duration, MIN_DURATION);

      // Check duration threshold
      if (durationSecs < effectiveThreshold) {
        return false;
      }

      // Check if any notification channel is enabled
      const hasOsNotification = config.os !== "never";
      const hasSound = config.sound !== "none";

      return hasOsNotification || hasSound;
    },
    [],
  );

  const notify = useCallback(
    async (payload: NotificationPayload, config: EventNotificationConfig) => {
      const settings = settingsRef.current;
      if (!settings) return;

      const channels = [
        shouldSendOsNotification(config) && "os",
        config.sound !== "none" && "sound",
      ].filter(Boolean);

      logger.log("sending notification", payload.title, "via", channels.join(", "));

      if (shouldSendOsNotification(config)) {
        sendOsNotification(payload);
      }

      if (config.sound !== "none") {
        playNotificationSound(config.sound, settings.volume);
      }
    },
    [],
  );

  const handleBlockStarted = useCallback((data: GrandCentralEvents["block-started"]) => {
    logger.debug("block-started", data.block_id);
    executionsRef.current.set(data.block_id, {
      blockId: data.block_id,
      runbookId: data.runbook_id,
      startTime: Date.now(),
    });
  }, []);

  const handleBlockFinished = useCallback(
    async (data: GrandCentralEvents["block-finished"]) => {
      logger.debug("block-finished", data.block_id, { success: data.success });
      const execution = executionsRef.current.get(data.block_id);
      executionsRef.current.delete(data.block_id);

      if (!execution) {
        logger.debug("no execution found for block", data.block_id);
        return;
      }

      // Skip block notification if this block is part of an active workflow
      if (serialExecutionsRef.current.has(execution.runbookId)) {
        logger.debug("skipping block notification - part of active workflow");
        return;
      }

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;
      const settings = settingsRef.current;
      if (!settings) return;

      const config = data.success ? settings.blockFinished : settings.blockFailed;

      logger.debug("checking notification", {
        durationSecs,
        success: data.success,
        config,
        hasFocus: document.hasFocus(),
      });

      if (!shouldNotify(durationSecs, config)) {
        logger.debug("filtered out by shouldNotify");
        return;
      }

      // Try to get runbook name for better context
      let runbookName = "Runbook";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore - use default name
      }

      const durationStr =
        durationSecs >= 60
          ? `${Math.floor(durationSecs / 60)}m ${Math.round(durationSecs % 60)}s`
          : `${durationSecs.toFixed(1)}s`;

      notify(
        {
          title: data.success ? "Block Completed" : "Block Failed",
          body: `${runbookName} - finished in ${durationStr}`,
          success: data.success,
          duration: durationSecs,
        },
        config,
      );
    },
    [shouldNotify, notify],
  );

  const handleBlockFailed = useCallback(
    async (data: GrandCentralEvents["block-failed"]) => {
      const execution = executionsRef.current.get(data.block_id);
      executionsRef.current.delete(data.block_id);

      if (!execution) return;

      // Skip block notification if this block is part of an active workflow
      if (serialExecutionsRef.current.has(execution.runbookId)) {
        logger.debug("skipping block-failed notification - part of active workflow");
        return;
      }

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;
      const settings = settingsRef.current;
      if (!settings) return;

      const config = settings.blockFailed;

      if (!shouldNotify(durationSecs, config)) return;

      let runbookName = "Runbook";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore
      }

      notify(
        {
          title: "Block Failed",
          body: `${runbookName}: ${data.error}`,
          success: false,
          duration: durationSecs,
        },
        config,
      );
    },
    [shouldNotify, notify],
  );

  const handleBlockCancelled = useCallback((data: GrandCentralEvents["block-cancelled"]) => {
    // Just clean up tracking, don't notify on cancellation
    executionsRef.current.delete(data.block_id);
  }, []);

  // Serial execution handlers
  const handleSerialStarted = useCallback((data: { runbook_id: string }) => {
    logger.debug("serial-started", data.runbook_id);
    serialExecutionsRef.current.set(data.runbook_id, {
      runbookId: data.runbook_id,
      startTime: Date.now(),
    });
  }, []);

  const handleSerialCompleted = useCallback(
    async (data: { runbook_id: string }) => {
      logger.debug("serial-completed", data.runbook_id);
      const execution = serialExecutionsRef.current.get(data.runbook_id);
      serialExecutionsRef.current.delete(data.runbook_id);

      if (!execution) {
        logger.debug("no execution found for serial", data.runbook_id);
        return;
      }

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;
      const settings = settingsRef.current;
      if (!settings) return;

      const config = settings.serialFinished;

      if (!shouldNotify(durationSecs, config)) {
        logger.debug("filtered out by shouldNotify");
        return;
      }

      let runbookName = "Workflow";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore
      }

      const durationStr =
        durationSecs >= 60
          ? `${Math.floor(durationSecs / 60)}m ${Math.round(durationSecs % 60)}s`
          : `${durationSecs.toFixed(1)}s`;

      notify(
        {
          title: "Workflow Completed",
          body: `${runbookName} - finished in ${durationStr}`,
          success: true,
          duration: durationSecs,
        },
        config,
      );
    },
    [shouldNotify, notify],
  );

  const handleSerialFailed = useCallback(
    async (data: { runbook_id: string; error: string }) => {
      logger.debug("serial-failed", data.runbook_id);
      const execution = serialExecutionsRef.current.get(data.runbook_id);
      serialExecutionsRef.current.delete(data.runbook_id);

      if (!execution) return;

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;
      const settings = settingsRef.current;
      if (!settings) return;

      const config = settings.serialFailed;

      if (!shouldNotify(durationSecs, config)) return;

      let runbookName = "Workflow";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore
      }

      const durationStr =
        durationSecs >= 60
          ? `${Math.floor(durationSecs / 60)}m ${Math.round(durationSecs % 60)}s`
          : `${durationSecs.toFixed(1)}s`;

      notify(
        {
          title: "Workflow Failed",
          body: `${runbookName} - failed after ${durationStr}: ${data.error}`,
          success: false,
          duration: durationSecs,
        },
        config,
      );
    },
    [shouldNotify, notify],
  );

  useEffect(() => {
    // Load initial settings
    refreshSettings();

    // Refresh settings periodically in case user changes them
    const settingsInterval = setInterval(refreshSettings, 5000);

    // Subscribe to block events
    const unsubStarted = grandCentral.on("block-started", handleBlockStarted);
    const unsubFinished = grandCentral.on("block-finished", handleBlockFinished);
    const unsubFailed = grandCentral.on("block-failed", handleBlockFailed);
    const unsubCancelled = grandCentral.on("block-cancelled", handleBlockCancelled);

    // Subscribe to serial execution events
    const unsubSerialStarted = onSerialExecutionStarted(handleSerialStarted);
    const unsubSerialCompleted = onSerialExecutionCompleted(handleSerialCompleted);
    const unsubSerialFailed = onSerialExecutionFailed(handleSerialFailed);

    return () => {
      clearInterval(settingsInterval);
      unsubStarted();
      unsubFinished();
      unsubFailed();
      unsubCancelled();
      unsubSerialStarted();
      unsubSerialCompleted();
      unsubSerialFailed();
    };
  }, [
    refreshSettings,
    handleBlockStarted,
    handleBlockFinished,
    handleBlockFailed,
    handleBlockCancelled,
    handleSerialStarted,
    handleSerialCompleted,
    handleSerialFailed,
  ]);

  // This component doesn't render anything
  return null;
}
